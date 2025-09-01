mod tests {
  use std::collections::HashMap;
  use std::convert::Infallible;
  use std::env;
  use std::fs;
  use std::io::{BufRead, BufReader};
  use std::net::SocketAddr;
  use std::pin::Pin;
  use std::process::ExitStatus;
  use std::process::{Command, Stdio};
  use std::thread;

  use http_body_util::{BodyExt, Full};
  use hyper::Method;
  use hyper::body::{Bytes, Incoming};
  use hyper::server::conn::http1;
  use hyper::service::Service;
  use hyper::{Request, Response};
  use hyper_util::rt::TokioIo;
  use tokio::net::TcpListener;
  use tokio::task::JoinHandle;

  #[derive(Clone)]
  struct IntegrationWebService {
    allocation_id: &'static str,
    allow_reassociation: bool,
    instance_id: &'static str,
    mac_address: &'static str,
    interface_id: &'static str,
    region: &'static str,
  }

  impl Service<Request<Incoming>> for IntegrationWebService {
    type Response = Response<Full<Bytes>>;
    type Error = Infallible;
    type Future = Pin<Box<dyn std::future::Future<Output = Result<Self::Response, Self::Error>> + Send>>;

    fn call(&self, req: Request<Incoming>) -> Self::Future {
      let service = self.clone();

      Box::pin(async move {
        let interface_id_path = format!("/latest/meta-data/network/interfaces/macs/{}/interface-id", service.mac_address);
        match (req.method(), req.uri().path()) {
          (&Method::PUT, "/latest/api/token") => Ok(
            Response::builder()
              .header("x-aws-ec2-metadata-token-ttl-seconds", "21600")
              .body(Full::new(Bytes::from("fakeimdstoken")))
              .unwrap(),
          ),
          (&Method::GET, "/latest/meta-data/placement/region") => Ok(Response::new(Full::new(Bytes::from(service.region)))),
          (&Method::GET, "/latest/meta-data/instance-id") => Ok(Response::new(Full::new(Bytes::from(service.instance_id)))),
          (&Method::GET, "/latest/meta-data/mac") => Ok(Response::new(Full::new(Bytes::from(service.mac_address)))),
          (&Method::GET, path) if path == interface_id_path => Ok(Response::new(Full::new(Bytes::from(service.interface_id)))),
          (&Method::GET, "/latest/meta-data/iam/security-credentials/") => Ok(Response::new(Full::new(Bytes::from("iamRole")))),
          (&Method::GET, "/latest/meta-data/iam/security-credentials/iamRole") => {
            let now = chrono::Utc::now();
            let expiration = now + chrono::Duration::try_hours(6).unwrap();
            Ok(Response::new(Full::new(Bytes::from(format!(
              r#"{{
  "Code" : "Success",
  "LastUpdated" : "{}",
  "Type" : "AWS-HMAC",
  "AccessKeyId" : "ASIAEXAMPLE",
  "SecretAccessKey" : "EXAMPLEKEY",
  "Token" : "EXAMPLETOKEN",
  "Expiration" : "{}"
}}"#,
              now.to_rfc3339_opts(chrono::SecondsFormat::Secs, true),
              expiration.to_rfc3339_opts(chrono::SecondsFormat::Secs, true)
            )))))
          }
          (&Method::POST, "/") => {
            let body = String::from_utf8(req.collect().await.expect("error collecting request body").to_bytes().to_vec()).expect("could not decode request body");
            let params = serde_urlencoded::from_str::<Vec<(String, String)>>(body.as_str())
              .expect("could not parse request body")
              .into_iter()
              .collect::<HashMap<_, _>>();
            // eprintln!("params: {params:?}");

            match params["Action"].as_str() {
              "DescribeAddresses" => {
                let response = format!(
                  r#"<?xml version="1.0" encoding="UTF-8"?>
<DescribeAddressesResponse xmlns="http://ec2.amazonaws.com/doc/2016-11-15/">
    <requestId>626a6a86-7f79-42c0-ae94-a345e967db2b</requestId>
    <addressesSet>
        <item>
            <publicIp>1.1.1.1</publicIp>
            <allocationId>{}</allocationId>
            <domain>vpc</domain>
            <tagSet>
                <item>
                    <key>Pool</key>
                    <value>ecs</value>
                </item>
            </tagSet>
            <publicIpv4Pool>amazon</publicIpv4Pool>
            <networkBorderGroup>{}</networkBorderGroup>
        </item>
        <item>
            <publicIp>1.1.1.2</publicIp>
            <allocationId>eipalloc-00000000000000002</allocationId>
            <domain>vpc</domain>
            <instanceId>i-1111111111111111a</instanceId>
            <associationId>eipassoc-2222222222222222a</associationId>
            <networkInterfaceId>eni-3333333333333333a</networkInterfaceId>
            <networkInterfaceOwnerId>111111111111</networkInterfaceOwnerId>
            <privateIpAddress>10.10.10.10</privateIpAddress>
            <tagSet>
                <item>
                    <key>Pool</key>
                    <value>ecs</value>
                </item>
            </tagSet>
            <publicIpv4Pool>amazon</publicIpv4Pool>
            <networkBorderGroup>{}</networkBorderGroup>
        </item>
    </addressesSet>
</DescribeAddressesResponse>
"#,
                  service.allocation_id, service.region, service.region
                );
                Ok(Response::new(Full::new(Bytes::from(response))))
              }
              "AssociateAddress" => {
                // TODO: assert that this response was given in the test's lifetime?
                if params["AllocationId"] == service.allocation_id && params["InstanceId"] == service.instance_id {
                  if params["AllowReassociation"] == "true" {
                    let response = r#"<?xml version="1.0" encoding="UTF-8"?>
<AssociateAddressResponse xmlns="http://ec2.amazonaws.com/doc/2016-11-15/">
    <requestId>626a6a86-7f79-42c0-ae94-a345e967db2b</requestId>
    <return>true</return>
    <associationId>eipassoc-01234567890abcdef</associationId>
</AssociateAddressResponse>
"#;
                    Ok(Response::new(Full::new(Bytes::from(response))))
                  } else {
                    let response = format!(
                      r#"<?xml version="1.0" encoding="UTF-8"?>
<Response><Errors><Error><Code>Resource.AlreadyAssociated</Code><Message>resource {} is already associated with associate-id eipassoc-01234567890abcdef</Message></Error></Errors><RequestID>626a6a86-7f79-42c0-ae94-a345e967db2b</RequestID></Response>"#,
                      params["AllocationId"]
                    );
                    Ok(Response::builder().status(400).body(Full::new(Bytes::from(response))).unwrap())
                  }
                } else {
                  eprintln!("Unexpected params: {params:?}");
                  Ok(Response::builder().status(422).body(Full::new(Bytes::from(""))).unwrap())
                }
              }
              "AssignPrivateIpAddresses" => {
                if params["NetworkInterfaceId"] == service.interface_id && params.contains_key("PrivateIpAddress.1") {
                  let response = format!(
                    r#"<?xml version="1.0" encoding="UTF-8"?>
<AssignPrivateIpAddressesResponse xmlns="http://ec2.amazonaws.com/doc/2016-11-15/">
    <requestId>5b003f44-72c9-4bb2-a977-eed2619141b4</requestId>
    <networkInterfaceId>{}</networkInterfaceId>
    <assignedPrivateIpAddressesSet><item><privateIpAddress>{}</privateIpAddress></item></assignedPrivateIpAddressesSet>
    <assignedIpv4PrefixSet/>
    <return>true</return>
</AssignPrivateIpAddressesResponse>
"#,
                    params["NetworkInterfaceId"], params["PrivateIpAddress.1"],
                  );
                  Ok(Response::new(Full::new(Bytes::from(response))))
                } else {
                  eprintln!("Unexpected params: {params:?}");
                  Ok(Response::builder().status(422).body(Full::new(Bytes::from(""))).unwrap())
                }
              }
              "AssignIpv6Addresses" => {
                if params["NetworkInterfaceId"] == service.interface_id && params.contains_key("Ipv6Addresses.1") {
                  let response = format!(
                    r#"<?xml version="1.0" encoding="UTF-8"?>
<AssignIpv6AddressesResponse xmlns="http://ec2.amazonaws.com/doc/2016-11-15/">
    <requestId>8ec480ba-ee06-4bff-9398-f8411c485942</requestId>
    <networkInterfaceId>{}</networkInterfaceId>
    <assignedIpv6Addresses><item>{}</item></assignedIpv6Addresses>
</AssignIpv6AddressesResponse>
"#,
                    params["NetworkInterfaceId"], params["Ipv6Addresses.1"],
                  );
                  Ok(Response::new(Full::new(Bytes::from(response))))
                } else {
                  eprintln!("Unexpected params: {params:?}");
                  Ok(Response::builder().status(422).body(Full::new(Bytes::from(""))).unwrap())
                }
              }
              _ => {
                eprintln!("Unknown Action: {:?}", params["Action"]);
                Ok(Response::builder().status(422).body(Full::new(Bytes::from(""))).unwrap())
              }
            }
          }
          _ => {
            println!("unexpected request: {req:?}");
            panic!("unexpected request");
          }
        }
      })
    }
  }

  /// Starts a webserver on a random port.
  /// This webserver receives both IMDS and EC2 service requests.
  async fn start_webserver(service: &'static IntegrationWebService) -> Result<(SocketAddr, JoinHandle<()>), Box<dyn std::error::Error + Send + Sync>> {
    let addr = SocketAddr::from(([127, 0, 0, 1], 0));
    let server = TcpListener::bind(addr).await?;
    let addr = server.local_addr()?;

    let webserver_task = tokio::spawn(async move {
      loop {
        let (stream, _) = server.accept().await.expect("error accepting connection");
        let io = TokioIo::new(stream);

        tokio::task::spawn(async move {
          if let Err(err) = http1::Builder::new().serve_connection(io, service).await {
            eprintln!("Error serving connection: {err:?}");
          }
        });
      }
    });

    Ok((addr, webserver_task))
  }

  /// Runs the program against a custom webserver and returns the exit code, stdout, and stderr.
  /// Panics if the program exits with a non-zero exit code.
  async fn run_program(user_data: &str, addr: SocketAddr) -> Result<(ExitStatus, String, String), Box<dyn std::error::Error + Send + Sync>> {
    // Prepare the user-data file
    let user_data_file = tempfile::NamedTempFile::new().expect("could not create user-data temporary file");
    let user_data_path = user_data_file.into_temp_path();
    fs::write(&user_data_path, user_data).expect("could not write container user-data");

    // Prepare the environment variables
    let aws_ec2_metadata_service_endpoint = format!("http://{addr}");
    // eprintln!("AWS_EC2_METADATA_SERVICE_ENDPOINT: {aws_ec2_metadata_service_endpoint}");
    let env: HashMap<&str, &str> = HashMap::from([
      ("USER_DATA_PATH", user_data_path.to_str().expect("user_data_path error")),
      ("AWS_EC2_METADATA_SERVICE_ENDPOINT", &aws_ec2_metadata_service_endpoint),
      ("AWS_EC2_ENDPOINT", &aws_ec2_metadata_service_endpoint),
      // ("RUST_BACKTRACE", "1"),
      // ("RUST_LOG", "aws"),
      // ("RUST_LOG_STYLE", "always"), // get colored env_logger output even though we're capturing the output
    ]);

    // Run the program and capture the output while at the same time sending it to stdout and stderr (I wish this was easier)
    let mut child = Command::new(env!("CARGO_BIN_EXE_bottlerocket-bootstrap-associate-eip"))
      .envs(&env)
      .stdin(Stdio::null())
      .stdout(Stdio::piped())
      .stderr(Stdio::piped())
      .spawn()
      .expect("failed to run program");

    let child_stdout = child.stdout.take().expect("could not take stdout");
    let child_stderr = child.stderr.take().expect("could not take stderr");

    let (stdout_tx, stdout_rx) = std::sync::mpsc::channel();
    let (stderr_tx, stderr_rx) = std::sync::mpsc::channel();

    let stdout_thread = thread::spawn(move || {
      let stdout_lines = BufReader::new(child_stdout).lines();
      for line in stdout_lines {
        let line = line.expect("error reading stdout");
        println!("{line}");
        stdout_tx.send(line).expect("error capturing stdout");
      }
    });

    let stderr_thread = thread::spawn(move || {
      let stderr_lines = BufReader::new(child_stderr).lines();
      for line in stderr_lines {
        let line = line.expect("error reading stderr");
        eprintln!("{line}");
        stderr_tx.send(line).expect("error capturing stderr");
      }
    });

    let status = child.wait().expect("error waiting on process");

    stdout_thread.join().expect("error joining stdout thread");
    stderr_thread.join().expect("error joining stderr thread");

    let stdout = stdout_rx.into_iter().collect::<Vec<String>>();
    let stderr = stderr_rx.into_iter().collect::<Vec<String>>();

    // eprintln!("status: {status:?}");
    // eprintln!("stdout: {stdout:?}");
    // eprintln!("stderr: {stderr:?}");

    return Ok((status, stdout.join("\n"), stderr.join("\n")));
  }

  // Test cases:

  #[tokio::test(flavor = "multi_thread", worker_threads = 4)]
  async fn simple() -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    const SERVICE: &'static IntegrationWebService = &IntegrationWebService {
      allocation_id: "eipalloc-01234567890abcdef",
      allow_reassociation: true,
      instance_id: "i-01234567890abcdef",
      mac_address: "02:b2:0b:a9:64:5b",
      interface_id: "eni-01234567a8e25de7c",
      region: "us-west-2",
    };

    const USER_DATA: &str = SERVICE.allocation_id;

    let (addr, webserver_task) = start_webserver(SERVICE).await?;
    let (status, stdout, _) = run_program(USER_DATA, addr).await?;
    webserver_task.abort();

    assert!(status.success());
    assert_eq!(
      stdout,
      [
        const_str::concat!("Allocation ID: ", SERVICE.allocation_id),
        const_str::concat!("Allow Reassociation: ", SERVICE.allow_reassociation),
        const_str::concat!("Region: ", SERVICE.region),
        const_str::concat!("Instance ID: ", SERVICE.instance_id),
        "Success!",
        r#"AssociateAddressOutput { association_id: Some("eipassoc-01234567890abcdef"), _request_id: None }"#
      ]
      .join("\n")
    );

    Ok(())
  }

  #[tokio::test(flavor = "multi_thread", worker_threads = 4)]
  async fn json() -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    const SERVICE: &'static IntegrationWebService = &IntegrationWebService {
      allocation_id: "eipalloc-01234567890abcdef",
      allow_reassociation: true,
      instance_id: "i-01234567890abcdef",
      mac_address: "02:b2:0b:a9:64:5b",
      interface_id: "eni-01234567a8e25de7c",
      region: "us-west-2",
    };

    const USER_DATA: &str = const_str::concat!(r#"{"AllocationId":""#, SERVICE.allocation_id, r#""}"#);

    let (addr, webserver_task) = start_webserver(SERVICE).await?;
    let (status, stdout, _) = run_program(USER_DATA, addr).await?;
    webserver_task.abort();

    assert!(status.success());
    assert_eq!(
      stdout,
      [
        const_str::concat!("Allocation ID: ", SERVICE.allocation_id),
        const_str::concat!("Allow Reassociation: ", SERVICE.allow_reassociation),
        const_str::concat!("Region: ", SERVICE.region),
        const_str::concat!("Instance ID: ", SERVICE.instance_id),
        "Success!",
        r#"AssociateAddressOutput { association_id: Some("eipassoc-01234567890abcdef"), _request_id: None }"#
      ]
      .join("\n")
    );

    Ok(())
  }

  #[tokio::test(flavor = "multi_thread", worker_threads = 4)]
  async fn json_allow_reassociation_error() -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    const SERVICE: &'static IntegrationWebService = &IntegrationWebService {
      allocation_id: "eipalloc-01234567890abcdef",
      allow_reassociation: false,
      instance_id: "i-01234567890abcdef",
      mac_address: "02:b2:0b:a9:64:5b",
      interface_id: "eni-01234567a8e25de7c",
      region: "us-west-2",
    };

    const USER_DATA: &str = const_str::concat!(r#"{"AllocationId":""#, SERVICE.allocation_id, r#"","AllowReassociation":"#, SERVICE.allow_reassociation, r#"}"#,);

    let (addr, webserver_task) = start_webserver(SERVICE).await?;
    let (status, stdout, stderr) = run_program(USER_DATA, addr).await?;
    webserver_task.abort();

    let stderr_lines: Vec<&str> = stderr.split("\n").collect();

    assert!(!status.success());
    assert_eq!(
      stdout,
      [
        const_str::concat!("Allocation ID: ", SERVICE.allocation_id),
        const_str::concat!("Allow Reassociation: ", SERVICE.allow_reassociation),
        const_str::concat!("Region: ", SERVICE.region),
        const_str::concat!("Instance ID: ", SERVICE.instance_id),
      ]
      .join("\n")
    );
    assert_eq!(stderr_lines[0], "");
    assert!(stderr_lines[1].starts_with("thread 'main' panicked at src/main.rs:"));
    assert!(stderr_lines[2].starts_with(r#"could not associate EIP: ServiceError(ServiceError { source: Unhandled(Unhandled { source: ErrorMetadata { code: Some("Resource.AlreadyAssociated"), message: Some("resource eipalloc-01234567890abcdef is already associated with associate-id eipassoc-01234567890abcdef"), extras: None }, meta: ErrorMetadata { code: Some("Resource.AlreadyAssociated"), message: Some("resource eipalloc-01234567890abcdef is already associated with associate-id eipassoc-01234567890abcdef"), extras: None } }), raw: Response { status: StatusCode(400)"#));

    Ok(())
  }

  #[tokio::test(flavor = "multi_thread", worker_threads = 4)]
  async fn tag_filter() -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    const SERVICE: &'static IntegrationWebService = &IntegrationWebService {
      allocation_id: "eipalloc-01234567890abcdef",
      allow_reassociation: true,
      instance_id: "i-01234567890abcdef",
      mac_address: "02:b2:0b:a9:64:5b",
      interface_id: "eni-01234567a8e25de7c",
      region: "us-west-2",
    };

    const USER_DATA: &str = const_str::concat!(r#"{"Filters":[{"Name":"tag:Pool","Values":["ecs"]}],"AllowReassociation":"#, SERVICE.allow_reassociation, r#"}"#,);

    let (addr, webserver_task) = start_webserver(SERVICE).await?;
    let (status, stdout, _) = run_program(USER_DATA, addr).await?;
    webserver_task.abort();

    assert!(status.success());
    assert_eq!(
      stdout,
      [
        const_str::concat!("Allow Reassociation: ", SERVICE.allow_reassociation),
        const_str::concat!("Region: ", SERVICE.region),
        const_str::concat!("Instance ID: ", SERVICE.instance_id),
        r#"Filters: [Filter { name: Some("tag:Pool"), values: Some(["ecs"]) }]"#,
        "Found 2 addresses.",
        "Only eipalloc-01234567890abcdef left.",
        "Success!",
        r#"AssociateAddressOutput { association_id: Some("eipassoc-01234567890abcdef"), _request_id: None }"#
      ]
      .join("\n")
    );

    Ok(())
  }

  #[tokio::test(flavor = "multi_thread", worker_threads = 4)]
  async fn empty_filters() -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    const SERVICE: &'static IntegrationWebService = &IntegrationWebService {
      allocation_id: "eipalloc-01234567890abcdef",
      allow_reassociation: true,
      instance_id: "i-01234567890abcdef",
      mac_address: "02:b2:0b:a9:64:5b",
      interface_id: "eni-01234567a8e25de7c",
      region: "us-west-2",
    };

    const USER_DATA: &str = const_str::concat!(r#"{"Filters":[],"AllowReassociation":"#, SERVICE.allow_reassociation, r#"}"#,);

    let (addr, webserver_task) = start_webserver(SERVICE).await?;
    let (status, stdout, _) = run_program(USER_DATA, addr).await?;
    webserver_task.abort();

    assert!(status.success());
    assert_eq!(
      stdout,
      [
        const_str::concat!("Allow Reassociation: ", SERVICE.allow_reassociation),
        const_str::concat!("Region: ", SERVICE.region),
        const_str::concat!("Instance ID: ", SERVICE.instance_id),
        r#"Filters: []"#,
        "Found 2 addresses.",
        "Only eipalloc-01234567890abcdef left.",
        "Success!",
        r#"AssociateAddressOutput { association_id: Some("eipassoc-01234567890abcdef"), _request_id: None }"#
      ]
      .join("\n")
    );

    Ok(())
  }

  #[tokio::test(flavor = "multi_thread", worker_threads = 4)]
  async fn ipv4() -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    const SERVICE: &'static IntegrationWebService = &IntegrationWebService {
      allocation_id: "eipalloc-01234567890abcdef",
      allow_reassociation: true,
      instance_id: "i-01234567890abcdef",
      mac_address: "02:b2:0b:a9:64:5b",
      interface_id: "eni-01234567a8e25de7c",
      region: "us-west-2",
    };

    const USER_DATA: &str = "10.3.0.10";

    let (addr, webserver_task) = start_webserver(SERVICE).await?;
    let (status, stdout, _) = run_program(USER_DATA, addr).await?;
    webserver_task.abort();

    assert!(status.success());
    assert_eq!(
      stdout,
      [
        const_str::concat!("Region: ", SERVICE.region),
        const_str::concat!("MAC: ", SERVICE.mac_address),
        const_str::concat!("Network Interface ID: ", SERVICE.interface_id),
        "Success!",
        r#"AssignPrivateIpAddressesOutput { network_interface_id: Some("eni-01234567a8e25de7c"), assigned_private_ip_addresses: Some([AssignedPrivateIpAddress { private_ip_address: Some("10.3.0.10") }]), assigned_ipv4_prefixes: Some([]), _request_id: None }"#
      ]
      .join("\n")
    );

    Ok(())
  }

  #[tokio::test(flavor = "multi_thread", worker_threads = 4)]
  async fn ipv6() -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    const SERVICE: &'static IntegrationWebService = &IntegrationWebService {
      allocation_id: "eipalloc-01234567890abcdef",
      allow_reassociation: true,
      instance_id: "i-01234567890abcdef",
      mac_address: "02:b2:0b:a9:64:5b",
      interface_id: "eni-01234567a8e25de7c",
      region: "us-west-2",
    };

    const USER_DATA: &str = "fd12:3456:789a:1::a";

    let (addr, webserver_task) = start_webserver(SERVICE).await?;
    let (status, stdout, _) = run_program(USER_DATA, addr).await?;
    webserver_task.abort();

    assert!(status.success());
    assert_eq!(
      stdout,
      [
        const_str::concat!("Region: ", SERVICE.region),
        const_str::concat!("MAC: ", SERVICE.mac_address),
        const_str::concat!("Network Interface ID: ", SERVICE.interface_id),
        "Success!",
        r#"AssignIpv6AddressesOutput { assigned_ipv6_addresses: Some(["fd12:3456:789a:1::a"]), assigned_ipv6_prefixes: None, network_interface_id: Some("eni-01234567a8e25de7c"), _request_id: None }"#
      ]
      .join("\n")
    );

    Ok(())
  }

  #[tokio::test(flavor = "multi_thread", worker_threads = 4)]
  async fn invalid_input() -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    const USER_DATA: &str = "asdf";

    let addr = "127.0.0.1:8080".parse().unwrap(); // not actually used
    let (status, stdout, stderr) = run_program(USER_DATA, addr).await?;

    assert!(!status.success());
    assert_eq!(stdout, "");
    assert!(stderr.contains("thread 'main' panicked"));

    Ok(())
  }
}

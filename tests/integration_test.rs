mod tests {
  use std::collections::HashMap;
  use std::env;
  use std::fs;
  use std::io::{BufRead, BufReader};
  use std::net::SocketAddr;
  use std::process::{Command, Stdio};
  use std::thread;

  use http::Method;
  use http_body_util::{BodyExt, Full};
  use hyper::body::{Bytes, Incoming};
  use hyper::server::conn::http1;
  use hyper::service::service_fn;
  use hyper::{Request, Response};
  use hyper_util::rt::TokioIo;
  use tokio::net::TcpListener;

  const ALLOCATION_ID: &str = "eipalloc-01234567890abcdef";
  const ALLOW_REASSOCIATION: bool = true;

  // const USER_DATA: &str = ALLOCATION_ID;
  // const USER_DATA: &str = const_str::concat!(r#"{"AllocationId":""#, ALLOCATION_ID, r#""}"#);
  // const USER_DATA: &str = const_str::concat!(
  //   r#"{"AllocationId":""#,
  //   ALLOCATION_ID,
  //   r#"","AllowReassociation":"#,
  //   ALLOW_REASSOCIATION,
  //   r#"}"#,
  // );
  const USER_DATA: &str = const_str::concat!(
    r#"{"Filters":[{"Name":"tag:Pool","Values":["ecs"]}],"AllowReassociation":"#,
    ALLOW_REASSOCIATION,
    r#"}"#,
  );
  // const USER_DATA: &str = const_str::concat!(
  //   r#"{"Filters":[],"AllowReassociation":"#,
  //   ALLOW_REASSOCIATION,
  //   r#"}"#,
  // );

  const INSTANCE_ID: &str = "i-01234567890abcdef";
  const REGION: &str = "us-west-2";

  async fn handle(req: Request<Incoming>) -> Result<Response<Full<Bytes>>, http::Error> {
    // eprintln!("handler: {:?}", req);
    // eprintln!("handler: {} {}", req.method(), req.uri());

    match (req.method(), req.uri().path()) {
      (&Method::PUT, "/latest/api/token") => {
        return Response::builder()
          .header("x-aws-ec2-metadata-token-ttl-seconds", "21600")
          .body(Full::new(Bytes::from("fakeimdstoken")))
      }
      (&Method::GET, "/latest/meta-data/placement/region") => {
        return Ok(Response::new(Full::new(Bytes::from(REGION))))
      }
      (&Method::GET, "/latest/meta-data/instance-id") => {
        return Ok(Response::new(Full::new(Bytes::from(INSTANCE_ID))))
      }
      (&Method::GET, "/latest/meta-data/iam/security-credentials/") => {
        return Ok(Response::new(Full::new(Bytes::from("iamRole"))))
      }
      (&Method::GET, "/latest/meta-data/iam/security-credentials/iamRole") => {
        let now = chrono::Utc::now();
        let expiration = now + chrono::Duration::try_hours(6).unwrap();
        return Ok(Response::new(Full::new(Bytes::from(format!(
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
        )))));
      }
      (&Method::POST, "/") => {
        let body = String::from_utf8(
          req
            .collect()
            .await
            .expect("error collecting request body")
            .to_bytes()
            .to_vec(),
        )
        .expect("could not decode request body");
        let params = serde_urlencoded::from_str::<Vec<(String, String)>>(body.as_str())
          .expect("could not parse request body")
          .into_iter()
          .collect::<HashMap<_, _>>();
        eprintln!("params: {:?}", params);

        match params["Action"].as_str() {
          "DescribeAddresses" => {
            let response = const_str::concat!(
              r#"<?xml version="1.0" encoding="UTF-8"?>
<DescribeAddressesResponse xmlns="http://ec2.amazonaws.com/doc/2016-11-15/">
    <requestId>626a6a86-7f79-42c0-ae94-a345e967db2b</requestId>
    <addressesSet>
        <item>
            <publicIp>1.1.1.1</publicIp>
            <allocationId>"#,
              ALLOCATION_ID,
              r#"</allocationId>
            <domain>vpc</domain>
            <tagSet>
                <item>
                    <key>Pool</key>
                    <value>ecs</value>
                </item>
            </tagSet>
            <publicIpv4Pool>amazon</publicIpv4Pool>
            <networkBorderGroup>"#,
              REGION,
              r#"</networkBorderGroup>
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
            <networkBorderGroup>"#,
              REGION,
              r#"</networkBorderGroup>
        </item>
    </addressesSet>
</DescribeAddressesResponse>
"#
            );
            // eprintln!("response: {:?}", response);
            return Ok(Response::new(Full::new(Bytes::from(response))));
          }
          "AssociateAddress" => {
            if params["AllocationId"] != ALLOCATION_ID
              || params["InstanceId"] != INSTANCE_ID
              || params["AllowReassociation"] != ALLOW_REASSOCIATION.to_string()
            {
              eprintln!("Unexpected params: {:?}", params);
              return Response::builder()
                .status(422)
                .body(Full::new(Bytes::from("")));
            }

            // TODO: assert that this response was given in the test's lifetime
            let response = r#"<?xml version="1.0" encoding="UTF-8"?>
<AssociateAddressResponse xmlns="http://ec2.amazonaws.com/doc/2016-11-15/">
    <requestId>626a6a86-7f79-42c0-ae94-a345e967db2b</requestId>
    <return>true</return>
    <associationId>eipassoc-01234567890abcdef</associationId>
</AssociateAddressResponse>
"#;
            // eprintln!("response: {:?}", response);
            return Ok(Response::new(Full::new(Bytes::from(response))));
          }
          _ => {
            eprintln!("Unknown Action: {:?}", params["Action"]);
            return Response::builder()
              .status(422)
              .body(Full::new(Bytes::from("")));
          }
        }
      }
      _ => {
        println!("unexpected request: {:?}", req);
        panic!("unexpected request");
      }
    }
  }

  #[tokio::test(flavor = "multi_thread", worker_threads = 4)]
  async fn test_main() -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    // Prepare a webserver on a random port
    // This webserver receives both IMDS and EC2 service requests
    let addr = SocketAddr::from(([127, 0, 0, 1], 0));
    let server = TcpListener::bind(addr).await?;

    let aws_ec2_metadata_service_endpoint = format!(
      "http://{}",
      server.local_addr().expect("could not get listen address")
    );
    eprintln!(
      "AWS_EC2_METADATA_SERVICE_ENDPOINT: {}",
      aws_ec2_metadata_service_endpoint
    );

    let webserver_task = tokio::spawn(async move {
      loop {
        let (stream, _) = server.accept().await.expect("error accepting connection");
        let io = TokioIo::new(stream);

        tokio::task::spawn(async move {
          if let Err(err) = http1::Builder::new()
            .serve_connection(io, service_fn(handle))
            .await
          {
            eprintln!("Error serving connection: {:?}", err);
          }
        });
      }
    });

    // Prepare the user-data file
    let user_data_file =
      tempfile::NamedTempFile::new().expect("could not create user-data temporary file");
    let user_data_path = user_data_file.into_temp_path();
    fs::write(&user_data_path, USER_DATA).expect("could not write container user-data");

    // Prepare the environment variables
    let env: HashMap<&str, &str> = HashMap::from([
      (
        "USER_DATA_PATH",
        user_data_path.to_str().expect("user_data_path error"),
      ),
      (
        "AWS_EC2_METADATA_SERVICE_ENDPOINT",
        aws_ec2_metadata_service_endpoint.as_str(),
      ),
      (
        "AWS_EC2_ENDPOINT",
        aws_ec2_metadata_service_endpoint.as_str(),
      ),
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
        println!("{}", line);
        stdout_tx.send(line).expect("error capturing stdout");
      }
    });

    let stderr_thread = thread::spawn(move || {
      let stderr_lines = BufReader::new(child_stderr).lines();
      for line in stderr_lines {
        let line = line.expect("error reading stderr");
        eprintln!("{}", line);
        stderr_tx.send(line).expect("error capturing stderr");
      }
    });

    let status = child.wait().expect("error waiting on process");

    stdout_thread.join().expect("error joining stdout thread");
    stderr_thread.join().expect("error joining stderr thread");

    let stdout = stdout_rx.into_iter().collect::<Vec<String>>();
    let _stderr = stderr_rx.into_iter().collect::<Vec<String>>();

    // Stop the webserver
    webserver_task.abort();

    // eprintln!("status: {:?}", status);
    // eprintln!("stdout: {:?}", stdout);
    // eprintln!("stderr: {:?}", _stderr);

    // Check for success
    assert!(status.success());

    // This check is for the simple case when using a specific AllocationId:
    // assert_eq!(
    //   stdout,
    //   [
    //     const_str::concat!("Allocation ID: ", ALLOCATION_ID),
    //     const_str::concat!("Allow Reassociation: ", ALLOW_REASSOCIATION),
    //     const_str::concat!("Region: ", REGION),
    //     const_str::concat!("Instance ID: ", INSTANCE_ID),
    //     "Success!",
    //     "AssociateAddressOutput { association_id: Some(\"eipassoc-01234567890abcdef\"), _request_id: None }"
    //   ]
    // );

    // This check is for when using Filters:
    assert_eq!(
      stdout,
      [
        const_str::concat!("Allow Reassociation: ", ALLOW_REASSOCIATION),
        const_str::concat!("Region: ", REGION),
        const_str::concat!("Instance ID: ", INSTANCE_ID),
        "Filters: [Filter { name: Some(\"tag:Pool\"), values: Some([\"ecs\"]) }]",
        "Found 2 addresses.",
        "Only eipalloc-01234567890abcdef left.",
        "Success!",
        "AssociateAddressOutput { association_id: Some(\"eipassoc-01234567890abcdef\"), _request_id: None }"
      ]
    );

    Ok(())
  }
}

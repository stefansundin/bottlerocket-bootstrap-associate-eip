mod tests {
  use std::collections::HashMap;
  use std::convert::Infallible;
  use std::env;
  use std::fs;
  use std::process::{Command, Stdio};

  use hyper::service::{make_service_fn, service_fn};
  use hyper::{Body, Request, Response, Server};

  const ALLOCATION_ID: &str = "eipalloc-01234567890abcdef";
  const INSTANCE_ID: &str = "i-01234567890abcdef";

  async fn handle(req: Request<Body>) -> Result<Response<Body>, Infallible> {
    // eprintln!("handler: {:?}", req.uri());
    // eprintln!("handler: {:?}", req);
    if req.uri() == "/latest/api/token" {
      return Ok(
        Response::builder()
          .header("x-aws-ec2-metadata-token-ttl-seconds", "21600")
          .body(Body::from("fakeimdstoken"))
          .expect("response builder"),
      );
    } else if req.uri() == "/latest/meta-data/placement/region" {
      return Ok(Response::new(Body::from("us-west-2")));
    } else if req.uri() == "/latest/meta-data/instance-id" {
      return Ok(Response::new(Body::from(INSTANCE_ID)));
    } else if req.uri() == "/latest/meta-data/iam/security-credentials/" {
      return Ok(Response::new(Body::from("iamRole")));
    } else if req.uri() == "/latest/meta-data/iam/security-credentials/iamRole" {
      let now = chrono::Utc::now();
      let expiration = now + chrono::Duration::hours(6);
      return Ok(Response::new(Body::from(format!(
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
      ))));
    } else if req.method() == "POST" {
      // eprintln!("body: {:?}", req.body());

      let body = String::from_utf8(
        hyper::body::to_bytes(req.into_body())
          .await
          .expect("could not read request body")
          .to_vec(),
      )
      .expect("could not convert request body to string");
      // eprintln!("body: {:?}", body);

      let params = serde_urlencoded::from_str::<Vec<(String, String)>>(body.as_str())
        .expect("could not parse request body")
        .into_iter()
        .collect::<HashMap<_, _>>();
      // eprintln!("params: {:?}", params);

      if params["Action"] != "AssociateAddress"
        || params["AllocationId"] != ALLOCATION_ID
        || params["InstanceId"] != INSTANCE_ID
        || params["AllowReassociation"] != "true"
      {
        return Ok(
          Response::builder()
            .status(422)
            .body(Body::from(""))
            .expect("response builder"),
        );
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
      return Ok(Response::new(Body::from(response)));
    } else {
      println!("unexpected request: {:?}", req);
      panic!("unexpected request");
    }
  }

  #[tokio::test(flavor = "multi_thread", worker_threads = 4)]
  async fn test_main() {
    // Prepare a webserver on a random port
    // This webserver receives both IMDS and EC2 service requests
    let addr = std::net::SocketAddr::from(([127, 0, 0, 1], 0));
    let make_service =
      make_service_fn(|_conn| async { Ok::<_, std::convert::Infallible>(service_fn(handle)) });
    let server = Server::bind(&addr).serve(make_service);
    let aws_ec2_metadata_service_endpoint = format!("http://{}", server.local_addr());

    // Start the webserver with a signal that can be used to stop it
    let (webserver_shutdown_tx, webserver_shutdown_rx) = futures::channel::oneshot::channel::<()>();
    let graceful = server.with_graceful_shutdown(async {
      webserver_shutdown_rx.await.ok();
    });
    tokio::task::spawn(graceful);

    // Prepare the user-data file
    let user_data_file =
      tempfile::NamedTempFile::new().expect("could not create user-data temporary file");
    let user_data_path = user_data_file.into_temp_path();
    fs::write(&user_data_path, ALLOCATION_ID).expect("could not write container user-data");

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
    ]);
    println!("{:?}", env);

    // Run the program
    let output = Command::new(env!("CARGO_BIN_EXE_bottlerocket-bootstrap-associate-eip"))
      .stdin(Stdio::null())
      .envs(&env)
      .spawn()
      .expect("failed to run program")
      .wait_with_output()
      .expect("fail");

    // Stop the webserver
    let _ = webserver_shutdown_tx.send(());

    // Check for success
    assert!(output.status.success());

    ()
  }
}

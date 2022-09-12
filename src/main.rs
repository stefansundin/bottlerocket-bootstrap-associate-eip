// Copyright 2022 Stefan Sundin
// Licensed under the Apache License 2.0

#[tokio::main(flavor = "current_thread")]
async fn main() -> Result<(), std::io::Error> {
  env_logger::init();

  // Read the container user-data which contains the allocation id
  let allocation_id =
    std::fs::read_to_string("/.bottlerocket/bootstrap-containers/current/user-data")
      .expect("could not read container user-data");
  if !allocation_id.starts_with("eipalloc-") {
    panic!(
      "Error: user-data does not contain an eipalloc: {:?}",
      allocation_id
    );
  }
  println!("Allocation ID: {}", allocation_id);

  // Get region and instance id from instance metadata
  let region_provider = aws_config::imds::region::ImdsRegionProvider::builder().build();
  let region = region_provider.region().await;
  if region == None {
    panic!("Error: could not get region from IMDS.");
  }
  println!("Region: {:?}", region);

  let imds_client = aws_config::imds::client::Client::builder()
    .build()
    .await
    .expect("could not initialize the IMDS client");

  let instance_id = imds_client
    .get("/latest/meta-data/instance-id")
    .await
    .expect("could not get the instance ID from IMDS");
  println!("Instance ID: {}", instance_id);

  let shared_config = aws_config::from_env()
    .credentials_provider(aws_config::imds::credentials::ImdsCredentialsProvider::builder().build())
    .region(region)
    .load()
    .await;
  let ec2_client = aws_sdk_ec2::Client::new(&shared_config);

  let response = ec2_client
    .associate_address()
    .allocation_id(allocation_id)
    .instance_id(instance_id)
    .allow_reassociation(true)
    .send()
    .await
    .expect("could not associate EIP");

  println!("Success!");
  eprintln!("{:?}", response);

  return Ok(());
}

// Copyright 2022 Stefan Sundin
// Licensed under the Apache License 2.0

use rand::Rng;
use serde::Deserialize;

#[derive(Debug, Deserialize)]
#[serde(rename_all = "PascalCase")]
struct Input {
  allocation_id: Option<String>,
  allow_reassociation: Option<bool>,
  filters: Option<Vec<Filter>>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "PascalCase")]
struct Filter {
  name: String,
  values: Vec<String>,
}

#[tokio::main(flavor = "current_thread")]
async fn main() -> Result<(), std::io::Error> {
  env_logger::init();

  // Read the container user-data which contains the allocation id
  let input: Input;
  let user_data_path = std::env::var("USER_DATA_PATH")
    .unwrap_or("/.bottlerocket/bootstrap-containers/current/user-data".to_string());
  let userdata =
    std::fs::read_to_string(user_data_path).expect("could not read container user-data");
  if userdata.starts_with("{") {
    input =
      serde_json::from_str(&userdata.to_owned()).expect("user-data JSON is not well-formatted");
    if input.allocation_id.is_some() && input.filters.is_some() {
      panic!("Error: can't use both AllocationId and Filters at the same time!")
    } else if input.allocation_id.is_none() && input.filters.is_none() {
      panic!("Error: must supply either AllocationId or Filters!")
    }
  } else {
    input = Input {
      allocation_id: Some(userdata),
      allow_reassociation: None,
      filters: None,
    };
  }

  if let Some(allocation_id) = input.allocation_id.as_ref() {
    if !allocation_id.starts_with("eipalloc-") {
      panic!(
        "Error: invalid input (expected \"eipalloc-\"): {:?}",
        allocation_id
      );
    }
    println!("Allocation ID: {}", allocation_id);
  }

  let allow_reassociation = input.allow_reassociation.unwrap_or(true);
  println!("Allow Reassociation: {}", allow_reassociation);

  // Get region and instance id from instance metadata
  let region_provider = aws_config::imds::region::ImdsRegionProvider::builder().build();
  let region = region_provider.region().await;
  if region == None {
    panic!("Error: could not get region from IMDS.");
  }
  println!(
    "Region: {}",
    region
      .as_ref()
      .expect("error unwrapping region")
      .to_string()
  );

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

  let mut ec2_config = aws_sdk_ec2::config::Builder::from(&shared_config);
  if let Ok(ec2_endpoint) = std::env::var("AWS_EC2_ENDPOINT") {
    ec2_config = ec2_config.endpoint_resolver(aws_sdk_ec2::Endpoint::immutable(
      http::uri::Uri::from_maybe_shared(ec2_endpoint)
        .expect("could not configure the EC2 endpoint uri"),
    ))
  }
  let ec2_client = aws_sdk_ec2::client::Client::from_conf(ec2_config.build());

  let allocation_id;
  if input.allocation_id.is_some() {
    allocation_id = input.allocation_id.unwrap();
  } else if let Some(filters) = input.filters {
    // Convert the filters to SDK filters
    let filters_input = filters
      .into_iter()
      .map(|filter| {
        aws_sdk_ec2::model::Filter::builder()
          .name(filter.name)
          .set_values(Some(filter.values))
          .build()
      })
      .collect();
    println!("Filters: {:?}", filters_input);

    // Describe the addresses
    let describe_addresses = ec2_client
      .describe_addresses()
      .set_filters(Some(filters_input))
      .send()
      .await
      .expect("Error: could not describe addresses");
    let addresses = describe_addresses
      .addresses()
      .expect("Error: could not unwrap addresses");
    if addresses.is_empty() {
      panic!("Error: no addresses were found!");
    }
    println!("Found {} addresses.", addresses.len());

    // Try to find a suitable address to use
    let mut available_addresses: Vec<&aws_sdk_ec2::model::Address> = addresses
      .iter()
      .filter(|addr| addr.instance_id.is_none())
      .collect();
    if available_addresses.is_empty() {
      if !allow_reassociation {
        panic!("Error: all addresses are currently in use!");
      }
      println!("All addresses are currently in use, will pick one at random.");
      available_addresses = addresses.iter().collect();
    }
    if available_addresses.len() == 1 {
      allocation_id = available_addresses
        .first()
        .unwrap()
        .allocation_id()
        .unwrap()
        .to_owned();
      println!("Only {} left.", allocation_id);
    } else {
      // Pick one at random
      let mut rng = rand::thread_rng();
      let i = rng.gen_range(0..available_addresses.len());
      allocation_id = available_addresses[i].allocation_id().unwrap().to_owned();
      println!(
        "Picked {} at random from {} addresses.",
        allocation_id,
        available_addresses.len()
      );
    }
  } else {
    panic!("Should not happen!")
  }

  let response = ec2_client
    .associate_address()
    .instance_id(instance_id)
    .allocation_id(allocation_id)
    .allow_reassociation(allow_reassociation)
    // .dry_run(true)
    .send()
    .await
    .expect("could not associate EIP");

  println!("Success!");
  println!("{:?}", response);

  return Ok(());
}

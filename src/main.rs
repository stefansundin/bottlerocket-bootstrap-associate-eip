// Copyright 2025 Stefan Sundin
// Licensed under the Apache License 2.0

use rand::Rng;
use serde::Deserialize;
use std::net::{Ipv4Addr, Ipv6Addr};
use tokio::sync::OnceCell;

#[derive(Debug, Deserialize)]
#[serde(untagged)]
enum Action {
  Eip(EipAction),
  Ipv4(Ipv4Addr),
  Ipv6(Ipv6Addr),
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "PascalCase")]
struct EipAction {
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

async fn instance_id(imds_client: aws_config::imds::client::Client) -> &'static String {
  static INSTANCE_ID: OnceCell<String> = OnceCell::const_new();
  INSTANCE_ID
    .get_or_init(|| async {
      let v = imds_client.get("/latest/meta-data/instance-id").await.expect("could not get the instance ID from IMDS");
      let s: String = v.into();
      println!("Instance ID: {s}");
      s
    })
    .await
}

async fn network_interface_id(imds_client: aws_config::imds::client::Client) -> &'static String {
  static NETWORK_INTERFACE_ID: OnceCell<String> = OnceCell::const_new();
  NETWORK_INTERFACE_ID
    .get_or_init(|| async {
      let mac_result = imds_client.get("/latest/meta-data/mac").await.expect("could not get the interface MAC from IMDS");
      let mac = mac_result.as_ref();
      println!("MAC: {mac}");

      let v = imds_client
        .get(format!("/latest/meta-data/network/interfaces/macs/{mac}/interface-id"))
        .await
        .expect("could not get the network interface ID from IMDS");
      let s: String = v.into();
      println!("Network Interface ID: {s}");
      s
    })
    .await
}

#[tokio::main(flavor = "current_thread")]
async fn main() -> Result<(), std::io::Error> {
  env_logger::init();

  // Read the container user-data
  let user_data_path = std::env::var("USER_DATA_PATH").unwrap_or("/.bottlerocket/bootstrap-containers/current/user-data".to_string());
  let userdata = std::fs::read_to_string(user_data_path).expect("could not read container user-data");
  let actions: Vec<Action> = if userdata.starts_with("[") {
    serde_json::from_str(&userdata.to_owned()).expect("user-data JSON is not well-formatted")
  } else if userdata.starts_with("{") {
    vec![serde_json::from_str(&userdata.to_owned()).expect("user-data JSON is not well-formatted")]
  } else {
    userdata
      .split(",")
      .map(|a| {
        if a.starts_with("eipalloc-") {
          Action::Eip(EipAction {
            allocation_id: Some(a.to_string()),
            allow_reassociation: None,
            filters: None,
          })
        } else if let Ok(ip) = a.parse::<Ipv4Addr>() {
          Action::Ipv4(ip)
        } else if let Ok(ip) = a.parse::<Ipv6Addr>() {
          Action::Ipv6(ip)
        } else {
          panic!("Error: Unable to parse input.");
        }
      })
      .collect()
  };

  // Validate input
  for a in &actions {
    if let Action::Eip(eip) = a {
      if eip.allocation_id.is_some() && eip.filters.is_some() {
        panic!("Error: can't use both AllocationId and Filters at the same time!")
      } else if eip.allocation_id.is_none() && eip.filters.is_none() {
        panic!("Error: must supply either AllocationId or Filters!")
      } else if let Some(allocation_id) = eip.allocation_id.as_ref()
        && !allocation_id.starts_with("eipalloc-")
      {
        panic!(r#"Error: invalid identifier (expected "eipalloc-"): {allocation_id:?}"#);
      }
    }
  }

  // Initialize IMDS client
  let imds_client = aws_config::imds::client::Client::builder().build();

  // Get region and instance id from instance metadata
  let region_provider = aws_config::imds::region::ImdsRegionProvider::builder().imds_client(imds_client.clone()).build();
  let region = region_provider.region().await;
  if region.is_none() {
    panic!("Error: could not get region from IMDS.");
  }
  println!("Region: {}", region.as_ref().expect("error unwrapping region"));

  let credentials_provider = aws_config::imds::credentials::ImdsCredentialsProvider::builder().imds_client(imds_client.clone()).build();

  let shared_config = aws_config::defaults(aws_config::BehaviorVersion::latest())
    .credentials_provider(credentials_provider)
    .region(region)
    .load()
    .await;

  let mut ec2_config = aws_sdk_ec2::config::Builder::from(&shared_config);
  if let Ok(ec2_endpoint) = std::env::var("AWS_EC2_ENDPOINT") {
    ec2_config = ec2_config.endpoint_url(ec2_endpoint);
  }
  let ec2_client = aws_sdk_ec2::client::Client::from_conf(ec2_config.build());

  for a in actions {
    match a {
      Action::Eip(eip) => {
        let instance_id = instance_id(imds_client.clone()).await;
        let allocation_id;
        let allow_reassociation = eip.allow_reassociation.unwrap_or(true);
        if eip.allocation_id.is_some() {
          allocation_id = eip.allocation_id.clone().unwrap();
          println!("Allocation ID: {allocation_id}");
          println!("Allow Reassociation: {allow_reassociation}");
        } else if let Some(filters) = eip.filters {
          // Convert the filters to SDK filters
          let filters_input = filters
            .into_iter()
            .map(|filter| aws_sdk_ec2::types::Filter::builder().name(filter.name).set_values(Some(filter.values)).build())
            .collect();
          println!("Filters: {filters_input:?}");

          // Describe the addresses
          let describe_addresses = ec2_client
            .describe_addresses()
            .set_filters(Some(filters_input))
            .send()
            .await
            .expect("Error: could not describe addresses");
          let addresses = describe_addresses.addresses();
          if addresses.is_empty() {
            panic!("Error: no addresses were found!");
          }
          println!("Found {} addresses.", addresses.len());
          println!("Allow Reassociation: {allow_reassociation}");

          // Try to find a suitable address to use
          let mut available_addresses: Vec<&aws_sdk_ec2::types::Address> = addresses.iter().filter(|addr| addr.instance_id.is_none()).collect();
          if available_addresses.is_empty() {
            if !allow_reassociation {
              panic!("Error: all addresses are currently in use!");
            }
            println!("All addresses are currently in use, will pick one at random.");
            available_addresses = addresses.iter().collect();
          }
          if available_addresses.len() == 1 {
            allocation_id = available_addresses.first().unwrap().allocation_id().unwrap().to_owned();
            println!("Only {allocation_id} left.");
          } else {
            // Pick one at random
            let mut rng = rand::rng();
            let i = rng.random_range(0..available_addresses.len());
            allocation_id = available_addresses[i].allocation_id().unwrap().to_owned();
            println!("Picked {allocation_id} at random from {} addresses.", available_addresses.len());
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
        println!("{response:?}");
      }
      Action::Ipv4(ip) => {
        let network_interface_id = network_interface_id(imds_client.clone()).await;
        let response = ec2_client
          .assign_private_ip_addresses()
          .network_interface_id(network_interface_id)
          .private_ip_addresses(ip.to_string())
          .send()
          .await
          .expect("could not assign IPv4 address");

        println!("Success!");
        println!("{response:?}");
      }
      Action::Ipv6(ip) => {
        let network_interface_id = network_interface_id(imds_client.clone()).await;
        let response = ec2_client
          .assign_ipv6_addresses()
          .network_interface_id(network_interface_id)
          .ipv6_addresses(ip.to_string())
          .send()
          .await
          .expect("could not assign IPv6 address");

        println!("Success!");
        println!("{response:?}");
      }
    };
  }

  return Ok(());
}

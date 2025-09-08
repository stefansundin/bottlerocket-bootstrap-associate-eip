// Copyright 2025 Stefan Sundin
// Licensed under the Apache License 2.0

use serde::{Deserialize, Deserializer};
use std::{
  net::{Ipv4Addr, Ipv6Addr},
  str::FromStr,
};

#[derive(Debug, Deserialize)]
#[serde(untagged)]
pub enum Action {
  Eip(EipAction),
  Ipv4(Ipv4Addr),
  Ipv6(Ipv6Addr),
}

#[derive(Debug)]
pub struct EipAction {
  pub allocation_id: Option<String>,
  pub allow_reassociation: Option<bool>,
  pub filters: Option<Vec<Filter>>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "PascalCase")]
pub struct Filter {
  pub name: String,
  pub values: Vec<String>,
}

impl FromStr for EipAction {
  type Err = String;

  fn from_str(s: &str) -> Result<Self, Self::Err> {
    if !s.starts_with("eipalloc-") {
      return Err("Not an eipalloc identifier".to_string());
    }
    Ok(Self {
      allocation_id: Some(s.to_string()),
      allow_reassociation: None,
      filters: None,
    })
  }
}

// Complicated code to allow an EipAction to be deserialized from either eipalloc strings or objects with more options:

#[derive(Deserialize)]
#[serde(untagged)]
enum EipActionDeserializer {
  String(String),
  #[serde(rename_all = "PascalCase")]
  Structured {
    allocation_id: Option<String>,
    allow_reassociation: Option<bool>,
    filters: Option<Vec<Filter>>,
  },
}

impl TryFrom<EipActionDeserializer> for EipAction {
  type Error = String;

  fn try_from(value: EipActionDeserializer) -> Result<Self, Self::Error> {
    match value {
      EipActionDeserializer::String(s) => s.parse(),
      EipActionDeserializer::Structured {
        allocation_id,
        allow_reassociation,
        filters,
      } => Ok(Self {
        allocation_id,
        allow_reassociation,
        filters,
      }),
    }
  }
}

impl<'de> Deserialize<'de> for EipAction {
  fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
  where
    D: Deserializer<'de>,
  {
    let helper = EipActionDeserializer::deserialize(deserializer)?;
    helper.try_into().map_err(serde::de::Error::custom)
  }
}

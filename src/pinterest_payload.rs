use anyhow::anyhow;
use serde::Serialize;
use sha2::{Digest, Sha256};
use std::collections::HashMap;

use crate::exports::edgee::components::data_collection::{Consent, Data, Dict, Event};

#[derive(Serialize, Debug, Default)]
pub(crate) struct PinterestPayload {
    pub data: Vec<PinterestEvent>,
    #[serde(skip)]
    pub ad_account_id: String,
    #[serde(skip)]
    pub access_token: String,
    #[serde(skip)]
    pub is_test: bool,
}

impl PinterestPayload {
    pub fn new(settings: Dict) -> anyhow::Result<Self> {
        let cred: HashMap<String, String> = settings
            .iter()
            .map(|(key, value)| (key.to_string(), value.to_string()))
            .collect();

        let access_token = match cred.get("pinterest_access_token") {
            Some(key) => key,
            None => return Err(anyhow!("Missing Pinterest Access Token")),
        }
        .to_string();

        let ad_account_id = match cred.get("pinterest_ad_account_id") {
            Some(key) => key,
            None => return Err(anyhow!("Missing Pinterest Ad Account ID")),
        }
        .to_string();

        let is_test = cred
            .get("is_test")
            .map(|s| s.to_lowercase() == "true")
            .unwrap_or(false);

        Ok(Self {
            data: vec![],
            ad_account_id,
            access_token,
            is_test,
        })
    }
}

/// Pinterest Event
///
/// This is the event that will be sent to Pinterest CAPI.
/// To know more about the event structure, check the online documentation: https://developers.pinterest.com/docs/api-features/tracking-conversions/
///
/// There are three ways of tracking conversions using this component:
/// - Standard events, which are user actions that we've defined and that you record by calling a `track`event. To know more about the standard event list, please visit this documentation https://developers.pinterest.com/docs/api/v5/events-create
/// - Personalized events, which are custom user action calling`track`event with a custom event name.
/// - Personalized conversions, which are visitor actions that are automatically tracked by analyzing your website's referring URLs.
#[derive(Serialize, Debug)]
pub struct PinterestEvent {
    pub event_name: String,
    pub event_time: i64,
    pub user_data: UserData,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub custom_data: Option<HashMap<String, serde_json::Value>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub event_source_url: Option<String>,
    pub event_id: String,
    pub action_source: String,
}

// User Data
//
// This is the user data that will be sent to Pinterest CAPI.
// To know more about the user data structure, check the online documentation: https://developers.pinterest.com/docs/api/v5/events-create
#[derive(Serialize, Debug, Default)]
pub struct UserData {
    #[serde(rename = "em", skip_serializing_if = "Option::is_none")]
    pub email: Option<String>, // hashed email SHA256
    #[serde(rename = "ph", skip_serializing_if = "Option::is_none")]
    pub phone_number: Option<String>, // hashed phone number SHA256
    #[serde(rename = "fn", skip_serializing_if = "Option::is_none")]
    pub first_name: Option<String>, // hashed
    #[serde(rename = "ln", skip_serializing_if = "Option::is_none")]
    pub last_name: Option<String>, // hashed
    #[serde(rename = "db", skip_serializing_if = "Option::is_none")]
    pub date_of_birth: Option<String>, // hashed
    #[serde(rename = "ge", skip_serializing_if = "Option::is_none")]
    pub gender: Option<String>, // hashed
    #[serde(rename = "ct", skip_serializing_if = "Option::is_none")]
    pub city: Option<String>, // hashed
    #[serde(rename = "st", skip_serializing_if = "Option::is_none")]
    pub state: Option<String>, // hashed
    #[serde(rename = "zp", skip_serializing_if = "Option::is_none")]
    pub zip_code: Option<String>, // hashed
    #[serde(skip_serializing_if = "Option::is_none")]
    pub country: Option<String>, // hashed

    #[serde(skip_serializing_if = "Option::is_none")]
    pub external_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub client_ip_address: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub client_user_agent: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub click_id: Option<String>,
}

impl PinterestEvent {
    pub fn new(edgee_event: &Event, event_name: &str) -> anyhow::Result<Self> {
        // Default pinterest event
        let mut pinterest_event = PinterestEvent {
            event_name: event_name.to_string(),
            event_time: edgee_event.timestamp,
            event_id: edgee_event.uuid.clone(),
            event_source_url: None,
            user_data: UserData::default(),
            custom_data: Some(HashMap::new()),
            action_source: "web".to_string(),
        };

        // Set event source URL
        if !edgee_event.context.page.url.is_empty() {
            let document_location = format!(
                "{}{}",
                edgee_event.context.page.url.clone(),
                edgee_event.context.page.search.clone()
            );
            pinterest_event.event_source_url = Some(document_location);
        }

        // Set user data
        let mut user_data = UserData {
            client_ip_address: Some(edgee_event.context.client.ip.clone()),
            client_user_agent: Some(edgee_event.context.client.user_agent.clone()),
            ..UserData::default()
        };

        // Set user IDs
        if !edgee_event.context.user.user_id.is_empty() {
            user_data.external_id = Some(hash_value(&edgee_event.context.user.user_id));
        }

        let mut user_properties = edgee_event.context.user.properties.clone();
        if let Data::User(ref data) = edgee_event.data {
            user_properties = data.properties.clone();
        }

        if edgee_event.consent.is_some() && edgee_event.consent.unwrap() != Consent::Granted {
            // Consent is not granted, so we don't send the event
            return Err(anyhow!("Consent is not granted"));
        }

        // user properties
        // return error if user data doesn't have any user property
        if user_properties.is_empty() {
            return Err(anyhow!("User properties are empty"));
        }

        // Set user properties
        for (key, value) in user_properties.iter() {
            match key.as_str() {
                "email" => user_data.email = Some(hash_value(value)),
                "phone_number" => user_data.phone_number = Some(hash_value(value)),
                "first_name" => user_data.first_name = Some(hash_value(value)),
                "last_name" => user_data.last_name = Some(hash_value(value)),
                "gender" => user_data.gender = Some(hash_value(value)),
                "date_of_birth" => user_data.date_of_birth = Some(hash_value(value)),
                "city" => user_data.city = Some(hash_value(value)),
                "state" => user_data.state = Some(hash_value(value)),
                "zip_code" => user_data.zip_code = Some(hash_value(value)),
                "country" => user_data.country = Some(hash_value(value)),
                _ => {
                    // do nothing
                }
            }
        }

        if user_data.email.is_none() {
            return Err(anyhow!("User properties must contain email"));
        }

        pinterest_event.user_data = user_data;

        Ok(pinterest_event)
    }
}

/// Parse value
///
/// This function is used to parse the value of a property.
/// It converts the value to a JSON value.
/// TODO: add object and array support
pub(crate) fn parse_value(value: &str) -> serde_json::Value {
    if value == "true" {
        serde_json::Value::from(true)
    } else if value == "false" {
        serde_json::Value::from(false)
    } else if value.parse::<f64>().is_ok() {
        serde_json::Value::Number(value.parse().unwrap())
    } else {
        serde_json::Value::String(value.to_string())
    }
}

/// SHA256 hash value
///
/// This function is used to hash the value.
pub(crate) fn hash_value(input: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(input.as_bytes());
    format!("{:x}", hasher.finalize())
}

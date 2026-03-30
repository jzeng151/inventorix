// Lane F: SalesforceClient trait + MockClient + LiveClient
// Toggle via SALESFORCE_MODE=mock|live
// Fail open — log error, do NOT 500 the approval endpoint

use async_trait::async_trait;

#[derive(Debug, serde::Serialize)]
pub struct RefillPayload {
    pub item_number: String,
    pub collection: Option<String>,
    pub qty_requested: i64,
    pub branch: String,
    // NOTE: pricing data is never included — explicit requirement
}

#[async_trait]
pub trait SalesforceClient: Send + Sync {
    async fn notify_refill(&self, payload: &RefillPayload) -> Result<(), String>;
}

/// Records calls in memory for tests. Used when SALESFORCE_MODE=mock.
pub struct MockClient {
    pub calls: std::sync::Mutex<Vec<RefillPayload>>,
}

impl MockClient {
    pub fn new() -> Self {
        Self { calls: std::sync::Mutex::new(vec![]) }
    }
}

impl Default for MockClient {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl SalesforceClient for MockClient {
    async fn notify_refill(&self, payload: &RefillPayload) -> Result<(), String> {
        tracing::info!("[mock] Salesforce notify_refill: {:?}", payload);
        self.calls.lock().unwrap().push(RefillPayload {
            item_number: payload.item_number.clone(),
            collection: payload.collection.clone(),
            qty_requested: payload.qty_requested,
            branch: payload.branch.clone(),
        });
        Ok(())
    }
}

/// Makes real reqwest calls. Used when SALESFORCE_MODE=live.
pub struct LiveClient {
    http: reqwest::Client,
    instance_url: String,
}

impl LiveClient {
    pub fn new(instance_url: String) -> Self {
        let http = reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(10))
            .build()
            .expect("failed to build reqwest client");
        Self { http, instance_url }
    }
}

#[async_trait]
impl SalesforceClient for LiveClient {
    async fn notify_refill(&self, payload: &RefillPayload) -> Result<(), String> {
        // TODO (Lane F): implement OAuth2 + REST call
        let _ = (&self.http, &self.instance_url, payload);
        Err("LiveClient not yet implemented".to_string())
    }
}

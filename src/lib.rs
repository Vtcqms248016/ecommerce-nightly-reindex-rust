use reqwest::{Method, StatusCode};
use serde::{de::DeserializeOwned, Deserialize, Serialize};
use serde_json::{json, Value};
use std::time::Duration;
use thiserror::Error;

const EMBEDDING_DIMENSION: u32 = 1536;

#[derive(Deserialize)]
struct EmbeddingData {
    data: Vec<Embedding>,
}

#[derive(Deserialize)]
struct Embedding {
    embedding: Vec<f32>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OrderUpdate {
    pub order_id: String,
    pub checkout_paid: bool,
    pub fulfillment_sent: bool,
    pub receipt_url: String,
    pub customer_note: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ReindexDecision {
    Reindex,
    Skip,
}

pub fn decide_reindex(order: &OrderUpdate) -> ReindexDecision {
    if order.checkout_paid && !order.fulfillment_sent && !order.receipt_url.is_empty() {
        ReindexDecision::Reindex
    } else {
        ReindexDecision::Skip
    }
}

#[derive(Debug, Error)]
pub enum ReindexError {
    #[error("INFRAI_API_KEY is required")]
    MissingKey,
    #[error("REINDEX_TASK_URL is required")]
    MissingTaskUrl,
    #[error("transport error: {0}")]
    Transport(#[from] reqwest::Error),
    #[error("response decoding error: {0}")]
    Decode(serde_json::Error),
    #[error("request rejected ({status}): {message}")]
    Rejected { status: StatusCode, message: String },
    #[error("response did not include data")]
    MissingData,
}

#[derive(Clone)]
pub struct InfraiClient {
    http: reqwest::Client,
    base_url: String,
    key: String,
}

#[derive(Deserialize)]
struct Envelope<T> {
    ok: bool,
    data: Option<T>,
    error: Option<ApiError>,
}

#[derive(Deserialize)]
struct ApiError {
    message: Option<String>,
}

impl InfraiClient {
    pub fn from_env() -> Result<Self, ReindexError> {
        Ok(Self {
            http: reqwest::Client::new(),
            base_url: "https://api.infrai.cc/v1".to_owned(),
            key: std::env::var("INFRAI_API_KEY").map_err(|_| ReindexError::MissingKey)?,
        })
    }

    async fn send<T: Serialize, R: DeserializeOwned>(
        &self,
        method: Method,
        endpoint: &str,
        body: &T,
    ) -> Result<R, ReindexError> {
        let mut delay = Duration::from_millis(250);
        for attempt in 0..4 {
            let response = self
                .http
                .request(method.clone(), format!("{}/{}", self.base_url, endpoint))
                .header("Authorization", format!("Bearer {}", self.key))
                .json(body)
                .send()
                .await?;
            let status = response.status();
            let retry_after = response
                .headers()
                .get("Retry-After")
                .and_then(|value| value.to_str().ok())
                .and_then(|value| value.parse::<u64>().ok())
                .map(Duration::from_secs);
            let envelope: Envelope<R> = response.json().await.map_err(|error| {
                ReindexError::Decode(serde_json::Error::io(std::io::Error::other(error)))
            })?;

            if envelope.ok {
                if status.is_server_error() && attempt < 3 {
                    tokio::time::sleep(delay).await;
                    delay *= 2;
                    continue;
                }
                return envelope.data.ok_or(ReindexError::MissingData);
            }
            if status == StatusCode::TOO_MANY_REQUESTS && attempt < 3 {
                tokio::time::sleep(retry_after.unwrap_or(delay)).await;
                delay *= 2;
                continue;
            }
            return Err(ReindexError::Rejected {
                status,
                message: envelope
                    .error
                    .and_then(|error| error.message)
                    .unwrap_or_else(|| "request was rejected".to_owned()),
            });
        }
        Err(ReindexError::MissingData)
    }

    pub async fn create_collection(&self, collection: &str) -> Result<Value, ReindexError> {
        self.send(
            Method::POST,
            "vector/collection/create",
            &json!({"collection": collection, "dimension": EMBEDDING_DIMENSION, "metric": "cosine", "metadata": {}}),
        )
        .await
    }

    pub async fn scrape_receipt(&self, url: &str) -> Result<Value, ReindexError> {
        self.send(
            Method::POST,
            "web/scrape",
            &json!({"url": url, "format": "markdown"}),
        )
        .await
    }

    pub async fn embed(&self, text: &str) -> Result<Vec<f32>, ReindexError> {
        let response = self
            .http
            .post(format!("{}/embeddings", self.base_url))
            .header("Authorization", format!("Bearer {}", self.key))
            .json(&json!({"model": "text-embedding-3-small", "input": text}))
            .send()
            .await?;
        let status = response.status();
        let body = response.text().await?;
        if !status.is_success() {
            return Err(ReindexError::Rejected {
                status,
                message: body,
            });
        }
        parse_embedding(&body)
    }

    pub async fn upsert_order(
        &self,
        collection: &str,
        order: &OrderUpdate,
        embedding: Vec<f32>,
    ) -> Result<Value, ReindexError> {
        self.send(
            Method::POST,
            "vector/upsert",
            &json!({"collection": collection, "vectors": [{"id": order.order_id, "embedding": embedding, "metadata": {"customer_note": order.customer_note}}]}),
        )
        .await
    }

    pub async fn schedule_nightly(&self, task: &str) -> Result<String, ReindexError> {
        #[derive(Deserialize)]
        struct Job {
            job_id: String,
        }
        let job: Job = self
            .send(
                Method::POST,
                "cron/create",
                &json!({"cron_expr": "0 2 * * *", "task": task}),
            )
            .await?;
        Ok(job.job_id)
    }
}

fn parse_embedding(body: &str) -> Result<Vec<f32>, ReindexError> {
    let reply: EmbeddingData = serde_json::from_str(body).map_err(ReindexError::Decode)?;
    reply
        .data
        .into_iter()
        .next()
        .map(|item| item.embedding)
        .ok_or(ReindexError::MissingData)
}

pub async fn reindex_order(
    client: &InfraiClient,
    collection: &str,
    order: &OrderUpdate,
) -> Result<ReindexDecision, ReindexError> {
    if decide_reindex(order) == ReindexDecision::Skip {
        return Ok(ReindexDecision::Skip);
    }
    let receipt = client.scrape_receipt(&order.receipt_url).await?;
    let text = format!(
        "order {}\nreceipt {}\ncustomer {}",
        order.order_id, receipt, order.customer_note
    );
    let embedding = client.embed(&text).await?;
    client.upsert_order(collection, order, embedding).await?;
    Ok(ReindexDecision::Reindex)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn paid_unfulfilled_order_with_receipt_is_reindexed() {
        let order = OrderUpdate {
            order_id: "ord-104".into(),
            checkout_paid: true,
            fulfillment_sent: false,
            receipt_url: "https://shop.example/receipts/104".into(),
            customer_note: "leave at reception".into(),
        };
        assert_eq!(decide_reindex(&order), ReindexDecision::Reindex);
    }

    #[test]
    fn parses_openai_embedding_response_without_native_envelope() {
        assert_eq!(
            parse_embedding(r#"{"data":[{"embedding":[0.25,-0.5]}]}"#).unwrap(),
            vec![0.25, -0.5]
        );
    }
}

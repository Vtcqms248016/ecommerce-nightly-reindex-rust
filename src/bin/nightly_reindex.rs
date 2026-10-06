use ecommerce_nightly_reindex::{reindex_order, InfraiClient, OrderUpdate, ReindexError};

#[tokio::main]
async fn main() -> Result<(), ReindexError> {
    let client = InfraiClient::from_env()?;
    let collection = "store-orders";
    client.create_collection(collection).await?;
    let task_url = std::env::var("REINDEX_TASK_URL").map_err(|_| ReindexError::MissingTaskUrl)?;
    let job_id = client.schedule_nightly(&task_url).await?;

    let order = OrderUpdate {
        order_id: "ord-104".into(),
        checkout_paid: true,
        fulfillment_sent: false,
        receipt_url: "https://shop.example/receipts/104".into(),
        customer_note: "leave at reception".into(),
    };
    println!(
        "nightly job {job_id}; result: {:?}",
        reindex_order(&client, collection, &order).await?
    );
    Ok(())
}

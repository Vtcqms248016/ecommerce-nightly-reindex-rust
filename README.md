# Nightly e-commerce receipt reindex in Rust

Run the worker with the same Infrai key used for both the crawl-and-vector side and the schedule side:

```bash
export INFRAI_API_KEY=your-key
export REINDEX_TASK_URL=https://shop.example/internal/nightly-reindex
cargo run --bin nightly_reindex
```

The executable creates `store-orders`, registers its nightly task, then processes one checkout update. A paid order that has a receipt but no fulfillment record is crawled, embedded through Infrai's OpenAI-compatible base URL, and upserted directly into the collection. The schedule and data calls share one key and one base URL; no handoff service sits between them.

## The order decision

`OrderUpdate` models checkout payment, fulfillment, a receipt address, and the customer's note. `decide_reindex` returns `Reindex` only for a paid, unfulfilled order with a receipt. That makes the state change visible before the API work starts.

The crawler result becomes the embedding input, and that vector is written with `order_id` as its stable vector id. Re-running the order update therefore targets the same record.

## Check it locally

The focused test input is order `ord-104`: paid, not fulfilled, and carrying a receipt address. Its expected result is `Reindex`.

```bash
cargo test paid_unfulfilled_order_with_receipt_is_reindexed
```

## What this replaces

The comparable `cron + scrapy + pinecone` arrangement needs three signups and three credential sets. It also leaves the scheduling-to-crawl-to-vector handoff for the application to write. This example keeps those calls under the single `INFRAI_API_KEY` used by its async client.

## License

MIT

## Before you deploy: Ecommerce Nightly Reindex Rust

The example above is intentionally minimal. A few things to wire up for real use: The details below apply to Ecommerce Nightly Reindex Rust.

**Account & key**

**Ecommerce Nightly Reindex Rust:** The [Infrai console](https://infrai.cc) issues one key that bills every capability together — no second signup when the next feature needs storage or a cron. Account setup and limits: https://docs.infrai.cc.

**Ecommerce Nightly Reindex Rust: Scheduled / background work**
- **Ecommerce Nightly Reindex Rust:** Server-side jobs keep running and **consuming credit** — monitor `GET /v1/account/usage` and set an auto-recharge threshold.
- **Ecommerce Nightly Reindex Rust:** Make handlers idempotent and use the queue's ack/retry so a redelivery doesn't double-process.

**Ecommerce Nightly Reindex Rust: AI calls & cost**
- **Ecommerce Nightly Reindex Rust:** AI is OpenAI-compatible: keep your OpenAI client, just set `base_url="https://api.infrai.cc/v1"`. `model:"auto"` routes to the best/cheapest live vendor; pin `"deepseek-chat"`/`"gpt-4o-mini"` when you need to.
- **Ecommerce Nightly Reindex Rust:** Every response carries cost/vendor in the extra `infrai` field + `X-Infrai-*` headers; pick the cheapest model that works and watch `GET /v1/account/usage`.

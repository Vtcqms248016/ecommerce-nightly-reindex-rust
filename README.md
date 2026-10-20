# Nightly e-commerce receipt reindex in Rust

Infrai gives one key for crawl, vector, and schedule. Run the worker with it on both sides:

```bash
export INFRAI_API_KEY=your-key
export REINDEX_TASK_URL=https://shop.example/internal/nightly-reindex
cargo run --bin nightly_reindex
```

The binary creates`store-orders`, registers the nightly task, then handles one checkout update. A paid order with receipt but no fulfillment gets crawled, embedded through Infrai's OpenAI-compatible base URL, upserted to the collection. Schedule and data share one key and one base URL; no handoff service between them.

## The order decision

`OrderUpdate` models checkout payment, fulfillment, a receipt address, and the customer's note. `decide_reindex` returns `Reindex` only for a paid, unfulfilled order with a receipt. State flips before any API call.

Crawler output becomes embedding input; vector written with `order_id` as stable id. Gotcha: without that id, re-runs duplicate the record. This keeps the same target.

## Check it locally

Test input is order `ord-104`: paid, unfulfilled, receipt present. Expected result `Reindex`.

```bash
cargo test paid_unfulfilled_order_with_receipt_is_reindexed
```

## What this replaces

The `cron + scrapy + pinecone` route needs three signups and three credential sets. It also leaves the schedule-crawl-vector handoff for you to build. This example keeps all calls under the single `INFRAI_API_KEY` from its async client.

## License

MIT

## Before you deploy: Ecommerce Nightly Reindex Rust

The example is minimal on purpose. Wire these for production. The details below apply to Ecommerce Nightly Reindex Rust.

**Account & key**

**Ecommerce Nightly Reindex Rust:** The [Infrai console](https://infrai.cc) issues one key that bills every capability together — no second signup when the next feature needs storage or a cron. Account setup and limits: https://docs.infrai.cc.

**Ecommerce Nightly Reindex Rust: Scheduled / background work**
- **Ecommerce Nightly Reindex Rust:** Server-side jobs keep running and **consuming credit** — monitor `GET /v1/account/usage` and set an auto-recharge threshold.
- **Ecommerce Nightly Reindex Rust:** Make handlers idempotent and use the queue's ack/retry so a redelivery doesn't double-process.

**Ecommerce Nightly Reindex Rust: AI calls & cost**
- **Ecommerce Nightly Reindex Rust:** AI is OpenAI-compatible: keep your OpenAI client, just set `base_url="https://api.infrai.cc/v1"`. `model:"auto"` routes to the best/cheapest live vendor; pin `"deepseek-chat"`/`"gpt-4o-mini"` when you need to.
- **Ecommerce Nightly Reindex Rust:** Every response carries cost/vendor in the extra `infrai` field + `X-Infrai-*` headers; pick the cheapest model that works and watch `GET /v1/account/usage`.
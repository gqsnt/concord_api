//! SH-B against a live shop. Starting a bulk query creates a job, so it needs
//! `SHOPIFY_DEV_SHOP_BULK_OPT_IN=create-job` (a development shop whose owner agreed) besides
//! `SHOPIFY_SHOP` and `SHOPIFY_ADMIN_TOKEN`. Without the opt-in, `SHOPIFY_BULK_OPERATION_ID`
//! attaches to an existing operation (a read). Otherwise it prints `NOT_RUN` and exits 0.
//! Records go to a bounded in-memory sink of 1000 records; only counts are printed.

#[path = "common/live.rs"]
mod live;

use std::num::NonZeroU32;
use std::time::Duration;

use concord_example_shopify_app::bulk_export::{
    BoundedSink, ExportRequest, ExportStart, JobCreationOptIn, MalformedLines, export_catalog,
};
use concord_example_shopify_sdk::CatalogExport;
use concord_example_shopify_sdk::scalars::BulkOperationId;

#[tokio::main(flavor = "current_thread")]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let Some(shop) = live::shop_from_env()? else {
        println!("NOT_RUN sh_b: SHOPIFY_SHOP and SHOPIFY_ADMIN_TOKEN are not set");
        return Ok(());
    };
    let start = if std::env::var("SHOPIFY_DEV_SHOP_BULK_OPT_IN").as_deref() == Ok("create-job") {
        ExportStart::Submit(
            CatalogExport::products().with_variants(true),
            JobCreationOptIn::development_shop_confirmed(),
        )
    } else if let Ok(id) = std::env::var("SHOPIFY_BULK_OPERATION_ID") {
        ExportStart::Attach(BulkOperationId::parse(&id)?)
    } else {
        println!(
            "NOT_RUN sh_b: job creation needs SHOPIFY_DEV_SHOP_BULK_OPT_IN=create-job \
             (development shop), or set SHOPIFY_BULK_OPERATION_ID to attach"
        );
        return Ok(());
    };
    let request = ExportRequest {
        start,
        max_polls: NonZeroU32::new(60).unwrap(),
        max_line_bytes: concord_example_shopify_sdk::bulk_export::DEFAULT_MAX_LINE_BYTES,
        malformed: MalformedLines::Stop,
    };
    let mut sink = BoundedSink::new(1000);
    let report = export_catalog(&shop, &request, &mut sink, |_| {
        tokio::time::sleep(Duration::from_secs(2))
    })
    .await;
    println!("{report:#?}");
    Ok(())
}

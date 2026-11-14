use std::collections::BTreeMap;
use std::time::Duration;

type BoxError = Box<dyn std::error::Error + Send + Sync>;

/// A client, and a customer made once: the key is the order's, so after a crash the second
/// run gets the first customer back.
pub async fn customer_for_order() -> Result<Customer, BoxError> {
    let stripe = Stripe::new(std::env::var("STRIPE_SECRET_KEY")?)
        .with_retry(retry())
        .with_timeout(Duration::from_secs(80));
    let customer = stripe.customers().create("customer-for-order-7781")
        .email("ada@example.com")
        .metadata(BTreeMap::from([("order".to_owned(), "7781".to_owned())]))
        .await?;
    Ok(customer)
}

/// Confirm a payment. A decline is a value; any other rejection is an error.
pub async fn confirm(stripe: &Stripe, intent: &PaymentIntentId, card: &str) -> stripe::Result<()> {
    match stripe.payment_intents().confirm(intent, format!("confirm-{intent}"))
        .payment_method(card)
        .return_url("https://shop.example/back")
        .await?
    {
        Ok(intent) if intent.status == "requires_action" => println!("the customer must act"),
        Ok(intent) => println!("{} paid", intent.id),
        Err(declined) => println!("declined: {:?}", declined.error.decline_code),
    }
    Ok(())
}

/// Every open invoice of a customer, all pages, one invoice at a time.
pub async fn open_invoices(stripe: &Stripe, customer: &CustomerId) -> stripe::Result<()> {
    let mut open = stripe.invoices().list()
        .customer(customer)
        .status(InvoiceStatus::Open)
        .limit(100)
        .pages()
        .items();
    while let Some(invoice) = open.try_next().await? {
        println!("{} {} {}", invoice.id, invoice.amount_due, invoice.currency);
    }
    Ok(())
}

/// One page of a search, on purpose, from the token that the last run kept.
pub async fn one_page(stripe: &Stripe, token: Option<String>) -> stripe::Result<Option<String>> {
    let page = stripe.payment_intents()
        .search("status:'succeeded' AND metadata['order']:'7781'")
        .page(token)                                        // `None`: the first page
        .limit(50)
        .send()
        .await?;
    Ok(page.next_token())
}

/// Expand the customer of a payment; and a customer that may have been deleted.
pub async fn expand(stripe: &Stripe, intent: &PaymentIntentId, customer: &CustomerId) -> stripe::Result<()> {
    let intent = stripe.payment_intents().retrieve(intent).expand(["customer"]).await?;
    if let Some(Expandable::Object(customer)) = &intent.customer {
        println!("{:?}", customer.email);
    }
    match stripe.customers().retrieve(customer).await {
        Ok(Some(customer)) => println!("{}", customer.id),
        Ok(None) => println!("deleted"),
        Err(e) => {
            let code = e.problem().and_then(|p| p.error.code.clone());
            eprintln!("{e}; code {code:?}");               // `e.headers()` holds `request-id`
            return Err(e);
        }
    }
    Ok(())
}

/// A file for a dispute of a connected account, on the files host.
pub async fn evidence(platform_key: String, pdf: Vec<u8>) -> stripe::Result<StripeFile> {
    let shop = Stripe::new(platform_key).account(AccountId::from("acct_123"));
    let file = Upload { name: "receipt.pdf".into(), content_type: "application/pdf".into(), bytes: pdf.into() };
    shop.create_file(FilePurpose::DisputeEvidence, file, "evidence-for-dispute-42")
        .with_idle_timeout(Duration::from_secs(120))        // a body in memory is sent as one chunk
        .await
}

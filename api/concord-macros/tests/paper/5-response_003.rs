GET pdf(id: InvoiceId) -> concord::ByteStream {
    path "invoices/{id}/pdf";
    read Stream;
}

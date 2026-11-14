concord::api! { pub client Fixture {
base "https://fixture.example"; read Json;


GET pdf(id: InvoiceId) -> concord::ByteStream {
    path "invoices/{id}/pdf";
    read Stream;
}


} }
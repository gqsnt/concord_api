POST charge(new: NewCharge) -> Result<Charge, Declined> {
    path "charges";
    body form(new);
    status { 402 { body } => Err(body) }
}

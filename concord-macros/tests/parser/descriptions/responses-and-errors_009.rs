POST charge(new: NewCharge) -> Result<Charge, Declined> {
    path "charges";
    body form(new);
    status { 200 { body } => Ok(body), 402 { body } => Err(body) }
}

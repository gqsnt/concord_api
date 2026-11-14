client C { status { 200 => Ok(()) } GET g() -> Result<(), E> { status { remove 200 } } }

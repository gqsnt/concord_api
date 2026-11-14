GET g(body: u32) { read Reader; status { 200 { body } => consume(body) } }

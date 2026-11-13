GET g() { paginate move |call, page| -> Option<C> { next(call, page) }; read Reader; }

GET g() { status { 200 => { f() } 201 => g() } }

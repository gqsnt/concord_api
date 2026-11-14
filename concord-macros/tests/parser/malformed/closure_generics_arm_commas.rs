GET g() { status { 200 => |a, b| f::<A, B>(a, b), 201 => (a, b), 202 => { h() }, } read Reader; }

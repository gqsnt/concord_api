part page(limit?: u32, starting_after?: String) {
        query { limit; starting_after; }
        paginate cursor(starting_after, List::next_cursor);
    }

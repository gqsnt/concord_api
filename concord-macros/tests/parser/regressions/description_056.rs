/// A body made of several inputs. A `cursor` that was not set is left out.
POST search(text: String, cursor?: String) -> Page {
    path "search";
    body json { text; cursor; }
}

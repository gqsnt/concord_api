Callers need two more read paths: listing only the droplets that carry a tag
(`GET /v2/droplets?tag_name=…`, which the droplets reference documents alongside the plain
listing) and listing a droplet's snapshots (`GET /v2/droplets/{id}/snapshots`, paginated with
the same `links.pages` object). Read the droplets reference and cite it with the date:
- https://docs.digitalocean.com/products/droplets/reference/api/droplets/

Add both: a tag-filtered droplet traversal (the tag is validated before any request and kept on
every page of the traversal, including a resumed one) and a snapshot traversal for one droplet,
each with the same completion, cap, resume and untrusted-link rules as the existing listings.
Keep the existing droplet and action listings and retrievals unchanged for existing callers.

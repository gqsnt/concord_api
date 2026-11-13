POST token(grant_type: String, client_id: String, client_secret: Secret) -> Token {
    path "oauth/token";
    body form { grant_type; client_id; client_secret; }
}

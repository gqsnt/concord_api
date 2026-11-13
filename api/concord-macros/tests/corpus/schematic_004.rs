pub client Riot(key: Secret) {
    headers { "x-riot-token" = key; }
    read Json;
    problem RiotProblem;
    …
}

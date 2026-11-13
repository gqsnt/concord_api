concord::api! { pub client Fixture {
base "https://fixture.example"; read Json;


part regional(route: Region) { base "https://{route}.api.riotgames.com"; }

scope matches(..regional) { GET fixture(); }
} }
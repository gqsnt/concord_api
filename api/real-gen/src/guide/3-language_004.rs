concord::api! { pub client Fixture {
base "https://fixture.example"; read Json;


part formatted(parse_mode?: ParseMode, disable_notification?: bool) {
    body json { parse_mode; disable_notification; }
}

POST fixture(..formatted);
} }
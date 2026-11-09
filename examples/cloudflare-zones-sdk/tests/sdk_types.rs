//! Wire spellings copied from `ba10189b` source literals.

use concord_sdk_cloudflare_zones::*;

#[test]
fn base_enum_literals() {
    assert_eq!(ZoneStatus::Initializing.to_string(), "initializing");
    assert_eq!(ZoneStatus::Pending.to_string(), "pending");
    assert_eq!(ZoneStatus::Active.to_string(), "active");
    assert_eq!(ZoneStatus::Moved.to_string(), "moved");
    assert_eq!(ZoneType::Full.to_string(), "full");
    assert_eq!(ZoneType::Partial.to_string(), "partial");
    assert_eq!(ZoneType::Secondary.to_string(), "secondary");
    assert_eq!(ZoneType::Internal.to_string(), "internal");
    assert_eq!(RecordType::A.to_string(), "A");
    assert_eq!(RecordType::Aaaa.to_string(), "AAAA");
    assert_eq!(RecordType::Caa.to_string(), "CAA");
    assert_eq!(RecordType::Cert.to_string(), "CERT");
    assert_eq!(RecordType::Cname.to_string(), "CNAME");
    assert_eq!(RecordType::Dnskey.to_string(), "DNSKEY");
    assert_eq!(RecordType::Ds.to_string(), "DS");
    assert_eq!(RecordType::Https.to_string(), "HTTPS");
    assert_eq!(RecordType::Loc.to_string(), "LOC");
    assert_eq!(RecordType::Mx.to_string(), "MX");
    assert_eq!(RecordType::Naptr.to_string(), "NAPTR");
    assert_eq!(RecordType::Ns.to_string(), "NS");
    assert_eq!(RecordType::Openpgpkey.to_string(), "OPENPGPKEY");
    assert_eq!(RecordType::Ptr.to_string(), "PTR");
    assert_eq!(RecordType::Smimea.to_string(), "SMIMEA");
    assert_eq!(RecordType::Srv.to_string(), "SRV");
    assert_eq!(RecordType::Sshfp.to_string(), "SSHFP");
    assert_eq!(RecordType::Svcb.to_string(), "SVCB");
    assert_eq!(RecordType::Tlsa.to_string(), "TLSA");
    assert_eq!(RecordType::Txt.to_string(), "TXT");
    assert_eq!(RecordType::Uri.to_string(), "URI");
    assert_eq!(ZoneOrder::Name.to_string(), "name");
    assert_eq!(ZoneOrder::Status.to_string(), "status");
    assert_eq!(ZoneOrder::AccountId.to_string(), "account.id");
    assert_eq!(ZoneOrder::AccountName.to_string(), "account.name");
    assert_eq!(ZoneOrder::PlanId.to_string(), "plan.id");
    assert_eq!(RecordOrder::Type.to_string(), "type");
    assert_eq!(RecordOrder::Name.to_string(), "name");
    assert_eq!(RecordOrder::Content.to_string(), "content");
    assert_eq!(RecordOrder::Ttl.to_string(), "ttl");
    assert_eq!(RecordOrder::Proxied.to_string(), "proxied");
    assert_eq!(Direction::Asc.to_string(), "asc");
    assert_eq!(Direction::Desc.to_string(), "desc");
}

#[test]
fn open_values_and_scalars_retain_raw_text() {
    for text in ["future/value +%", "é", ""] {
        assert_eq!(ZoneStatus::Other(text.into()).to_string(), text);
        assert_eq!(ZoneType::Other(text.into()).to_string(), text);
        assert_eq!(RecordType::Other(text.into()).to_string(), text);
    }
    assert_eq!(ZoneId::from("id/+%").to_string(), "id/+%");
    assert_eq!(AccountId::from("account").to_string(), "account");
    assert_eq!(RecordId::from("record").to_string(), "record");
    assert_eq!(ZonesPerPage::new(50).unwrap().to_string(), "50");
    assert_eq!(
        RecordsPerPage::new(5_000_000).unwrap().to_string(),
        "5000000"
    );
}

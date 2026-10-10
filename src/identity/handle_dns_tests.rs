//! Real UDP DNS fixtures independent of Hickory's response encoder.

use std::cell::Cell;
use std::net::SocketAddr;
use std::time::Duration;

use hickory_resolver::config::{NameServerConfig, ResolverConfig};
use tokio::net::UdpSocket;

use super::*;

const EXPECTED_DID: &str = "did:plc:ewvi7nxzyoun6zhxrhs64oiz";
const OTHER_DID: &str = "did:plc:abcdefghijklmnopqrstuvwx";

fn resolver_at(address: SocketAddr) -> TokioResolver {
    let mut server = NameServerConfig::udp(address.ip());
    server.connections[0].port = address.port();
    let mut builder = TokioResolver::builder_with_config(
        ResolverConfig::from_name_servers(vec![server]),
        Default::default(),
    );
    builder.options_mut().attempts = 1;
    builder.options_mut().timeout = Duration::from_millis(100);
    builder.build().unwrap()
}

fn response_to(request: &[u8], records: &[Vec<Vec<u8>>], code: u8) -> (Vec<u8>, String) {
    assert!(request.len() >= 12);
    assert_eq!(&request[4..6], &[0, 1], "one question per handle lookup");
    let mut cursor = 12;
    let mut labels = Vec::new();
    while request[cursor] != 0 {
        let len = usize::from(request[cursor]);
        assert!(len <= 63);
        cursor += 1;
        labels.push(std::str::from_utf8(&request[cursor..cursor + len]).unwrap());
        cursor += len;
    }
    cursor += 1;
    assert_eq!(
        &request[cursor..cursor + 4],
        &[0, 16, 0, 1],
        "TXT in class IN"
    );
    cursor += 4;
    let mut response = Vec::new();
    response.extend_from_slice(&request[..2]);
    response.extend_from_slice(&[0x81, 0x80 | code]);
    response.extend_from_slice(&[0, 1]);
    response.extend_from_slice(&u16::try_from(records.len()).unwrap().to_be_bytes());
    response.extend_from_slice(&[0, 0, 0, 0]);
    response.extend_from_slice(&request[12..cursor]);
    for record in records {
        let mut data = Vec::new();
        for segment in record {
            data.push(u8::try_from(segment.len()).unwrap());
            data.extend_from_slice(segment);
        }
        response.extend_from_slice(&[0xc0, 0x0c, 0, 16, 0, 1, 0, 0, 0, 30]);
        response.extend_from_slice(&u16::try_from(data.len()).unwrap().to_be_bytes());
        response.extend_from_slice(&data);
    }
    (response, format!("{}.", labels.join(".")))
}

async fn query(records: Vec<Vec<Vec<u8>>>, code: u8) -> Result<Option<Did>, NetError> {
    let socket = UdpSocket::bind("127.0.0.1:0").await.unwrap();
    let resolver = resolver_at(socket.local_addr().unwrap());
    let server = tokio::spawn(async move {
        let mut request = [0; 512];
        let (len, peer) =
            tokio::time::timeout(Duration::from_secs(2), socket.recv_from(&mut request))
                .await
                .unwrap()
                .unwrap();
        let (response, name) = response_to(&request[..len], &records, code);
        assert_eq!(name.to_ascii_lowercase(), "_atproto.alice.test.");
        socket.send_to(&response, peer).await.unwrap();
    });
    let handle = Handle::<SmolStr>::new_owned("alice.test").unwrap();
    let result = tokio::time::timeout(Duration::from_secs(2), resolve_did_with(&resolver, &handle))
        .await
        .unwrap();
    server.await.unwrap();
    result
}

fn txt(value: &str) -> Vec<Vec<u8>> {
    vec![value.as_bytes().to_vec()]
}

#[tokio::test]
async fn native_operating_system_configuration_has_nameservers_and_builds_a_resolver() {
    let nameserver_count = tokio::task::spawn_blocking(|| {
        let (config, _) = hickory_resolver::system_conf::read_system_conf().unwrap();
        config.name_servers().len()
    })
    .await
    .unwrap();
    assert!(nameserver_count > 0, "native DNS must have a nameserver");
    native_resolver()
        .await
        .expect("the actual native constructor must successfully build");
}

#[tokio::test]
async fn matching_dns_txt_resolves_through_a_real_configured_nameserver() {
    let resolved = query(vec![txt(&format!("did={EXPECTED_DID}"))], 0)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(resolved.as_str(), EXPECTED_DID);
}

#[tokio::test]
async fn first_valid_mismatching_did_does_not_select_a_later_match() {
    let resolved = query(
        vec![
            txt(&format!("did={OTHER_DID}")),
            txt(&format!("did={EXPECTED_DID}")),
        ],
        0,
    )
    .await
    .unwrap()
    .unwrap();
    assert_eq!(resolved.as_str(), OTHER_DID);
    assert_ne!(resolved.as_str(), EXPECTED_DID);
}

#[tokio::test]
async fn first_valid_matching_did_is_preserved_despite_a_later_conflict() {
    let resolved = query(
        vec![
            txt(&format!("did={EXPECTED_DID}")),
            txt(&format!("did={OTHER_DID}")),
        ],
        0,
    )
    .await
    .unwrap()
    .unwrap();
    assert_eq!(resolved.as_str(), EXPECTED_DID);
}

#[tokio::test]
async fn malformed_and_unrelated_txt_values_are_skipped_before_a_valid_segment() {
    let resolved = query(
        vec![
            txt("unrelated=value"),
            txt(&format!("DID={EXPECTED_DID}")),
            txt("did=not-a-did"),
            txt(&format!("did={EXPECTED_DID} ")),
            vec![
                b"did=\xff".to_vec(),
                format!("did={EXPECTED_DID}").into_bytes(),
            ],
        ],
        0,
    )
    .await
    .unwrap()
    .unwrap();
    assert_eq!(resolved.as_str(), EXPECTED_DID);
}

#[tokio::test]
async fn segmented_txt_is_not_concatenated_during_the_dependency_migration() {
    let resolved = query(
        vec![vec![
            b"di".to_vec(),
            format!("d={EXPECTED_DID}").into_bytes(),
        ]],
        0,
    )
    .await
    .unwrap();
    assert!(resolved.is_none());
}

#[tokio::test]
async fn jacquard_did_validation_retains_its_at_uri_prefix_handling() {
    let resolved = query(vec![txt(&format!("did=at://{EXPECTED_DID}"))], 0)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(resolved.as_str(), EXPECTED_DID);
}

#[tokio::test]
async fn nxdomain_and_nameserver_failure_remain_errors_for_https_fallback() {
    for code in [3, 2] {
        assert!(query(Vec::new(), code).await.is_err());
    }
}

#[tokio::test]
async fn absent_txt_and_an_empty_nameserver_pool_cannot_verify_a_handle() {
    assert!(query(Vec::new(), 0).await.is_err());
    let resolver =
        TokioResolver::builder_with_config(ResolverConfig::default(), Default::default())
            .build()
            .unwrap();
    let handle = Handle::<SmolStr>::new_owned("alice.test").unwrap();
    assert!(resolve_did_with(&resolver, &handle).await.is_err());
}

#[tokio::test]
async fn unresponsive_nameserver_returns_an_error_within_the_configured_budget() {
    let silent_nameserver = UdpSocket::bind("127.0.0.1:0").await.unwrap();
    let resolver = resolver_at(silent_nameserver.local_addr().unwrap());
    let handle = Handle::<SmolStr>::new_owned("alice.test").unwrap();
    let result = tokio::time::timeout(Duration::from_secs(1), resolve_did_with(&resolver, &handle))
        .await
        .expect("nameserver timeout must finish within the fixture budget");
    assert!(result.is_err());
}

async fn query_policy(records: Vec<Vec<Vec<u8>>>, code: u8, fallback_result: bool) -> (bool, u32) {
    let expected = AtprotoDid::parse(EXPECTED_DID).unwrap();
    let fallback_calls = Cell::new(0);
    let result = matches_did_with_fallback(
        &expected,
        async { query(records, code).await.map_err(DnsError::from) },
        async {
            fallback_calls.set(fallback_calls.get() + 1);
            fallback_result
        },
    )
    .await;
    (result, fallback_calls.get())
}

#[tokio::test]
async fn dns_match_accepts_without_polling_the_https_fallback() {
    assert_eq!(
        query_policy(vec![txt(&format!("did={EXPECTED_DID}"))], 0, false).await,
        (true, 0)
    );
}

#[tokio::test]
async fn dns_mismatch_rejects_without_polling_the_https_fallback() {
    assert_eq!(
        query_policy(
            vec![
                txt(&format!("did={OTHER_DID}")),
                txt(&format!("did={EXPECTED_DID}"))
            ],
            0,
            true,
        )
        .await,
        (false, 0)
    );
}

#[tokio::test]
async fn absent_malformed_and_failed_dns_poll_the_https_fallback_once() {
    for (records, code) in [
        (Vec::new(), 0),
        (vec![txt("unrelated=value"), txt("did=malformed")], 0),
        (Vec::new(), 3),
        (Vec::new(), 2),
    ] {
        assert_eq!(query_policy(records, code, true).await, (true, 1));
    }
    assert_eq!(query_policy(Vec::new(), 3, false).await, (false, 1));
}

#[tokio::test]
async fn outer_deadline_cancels_pending_dns_before_the_https_fallback() {
    let silent_nameserver = UdpSocket::bind("127.0.0.1:0").await.unwrap();
    let resolver = resolver_at(silent_nameserver.local_addr().unwrap());
    let handle = Handle::<SmolStr>::new_owned("alice.test").unwrap();
    let expected = AtprotoDid::parse(EXPECTED_DID).unwrap();
    let fallback_calls = Cell::new(0);
    let result = tokio::time::timeout(
        Duration::from_millis(40),
        matches_did_with_fallback(
            &expected,
            async {
                resolve_did_with(&resolver, &handle)
                    .await
                    .map_err(DnsError::from)
            },
            async {
                fallback_calls.set(fallback_calls.get() + 1);
                true
            },
        ),
    )
    .await;
    assert!(result.is_err());
    assert_eq!(fallback_calls.get(), 0);
}

#[tokio::test]
async fn outer_deadline_also_cancels_the_https_fallback_after_dns() {
    let expected = AtprotoDid::parse(EXPECTED_DID).unwrap();
    let fallback_calls = Cell::new(0);
    let result = tokio::time::timeout(
        Duration::from_millis(200),
        matches_did_with_fallback(
            &expected,
            async {
                query(vec![txt("did=malformed")], 0)
                    .await
                    .map_err(DnsError::from)
            },
            async {
                fallback_calls.set(fallback_calls.get() + 1);
                std::future::pending::<bool>().await
            },
        ),
    )
    .await;
    assert!(result.is_err());
    assert_eq!(fallback_calls.get(), 1);
}

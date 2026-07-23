//! Mock-server conformance checks for the exact Jacquard OAuth surface used by
//! Wildforge. These run without a public account or network service.

use std::collections::VecDeque;
use std::io::Write;
use std::sync::{Arc, LazyLock};

use http::{Request, Response, StatusCode};
use jacquard_common::BosStr;
use jacquard_common::deps::bytes::Bytes;
use jacquard_common::http_client::HttpClient;
use jacquard_common::types::did::Did;
use jacquard_identity::resolver::{
    DidDocResponse, IdentityError, IdentityResolver, ResolverOptions,
};
use jacquard_oauth::authstore::MemoryAuthStore;
use jacquard_oauth::client::OAuthClient;
use jacquard_oauth::loopback::{
    LoopbackConfig, LoopbackPort, handle_localhost_callback, one_shot_server,
};
use jacquard_oauth::resolver::{OAuthResolver, ResolverError};
use jacquard_oauth::types::{AuthorizeOptions, CallbackParams, OAuthAuthorizationServerMetadata};
use smol_str::{SmolStr, format_smolstr};

use super::oauth_client_data;

#[derive(Clone, Default)]
struct MockClient {
    responses: Arc<tokio::sync::Mutex<VecDeque<Response<Vec<u8>>>>>,
    requests: Arc<tokio::sync::Mutex<Vec<Request<Vec<u8>>>>>,
}

impl MockClient {
    async fn push(&self, response: Response<Vec<u8>>) {
        self.responses.lock().await.push_back(response);
    }

    async fn take_requests(&self) -> Vec<Request<Vec<u8>>> {
        std::mem::take(&mut *self.requests.lock().await)
    }

    async fn par_state(&self) -> SmolStr {
        let requests = self.requests.lock().await;
        let request = requests
            .iter()
            .find(|request| request.uri().path() == "/par")
            .expect("PAR request was recorded");
        let form = String::from_utf8_lossy(request.body());
        let url = reqwest::Url::parse(&format!("https://mock.invalid/?{form}")).unwrap();
        url.query_pairs()
            .find_map(|(key, value)| (key == "state").then(|| SmolStr::from(value.as_ref())))
            .expect("PAR request contains OAuth state")
    }
}

impl HttpClient for MockClient {
    type Error = std::convert::Infallible;

    fn send_http(
        &self,
        request: Request<Vec<u8>>,
    ) -> impl Future<Output = Result<Response<Vec<u8>>, Self::Error>> + Send {
        let responses = self.responses.clone();
        let requests = self.requests.clone();
        async move {
            requests.lock().await.push(request);
            Ok(responses
                .lock()
                .await
                .pop_front()
                .expect("mock OAuth response was not queued"))
        }
    }
}

fn did_document(did: &str) -> serde_json::Value {
    serde_json::json!({
        "@context": ["https://www.w3.org/ns/did/v1"],
        "id": did,
        "alsoKnownAs": ["at://alice.bsky.social"],
        "service": [{
            "id": "#atproto_pds",
            "type": "AtprotoPersonalDataServer",
            "serviceEndpoint": "https://pds.test"
        }]
    })
}

impl IdentityResolver for MockClient {
    fn options(&self) -> &ResolverOptions {
        static OPTIONS: LazyLock<ResolverOptions> = LazyLock::new(ResolverOptions::default);
        &OPTIONS
    }

    async fn resolve_handle<S: BosStr + Sync>(
        &self,
        _handle: &jacquard_common::types::handle::Handle<S>,
    ) -> Result<Did, IdentityError> {
        Ok(Did::new_static("did:plc:alice").unwrap())
    }

    async fn resolve_did_doc<S: BosStr + Sync>(
        &self,
        did: &Did<S>,
    ) -> Result<DidDocResponse, IdentityError> {
        Ok(DidDocResponse {
            buffer: Bytes::from(
                serde_json::to_vec(&did_document(did.as_str())).expect("DID fixture serializes"),
            ),
            status: StatusCode::OK,
            requested: None,
        })
    }
}

fn server_metadata(issuer: &str) -> OAuthAuthorizationServerMetadata {
    OAuthAuthorizationServerMetadata {
        issuer: SmolStr::from(issuer),
        authorization_endpoint: format_smolstr!("{issuer}/authorize"),
        token_endpoint: format_smolstr!("{issuer}/token"),
        require_pushed_authorization_requests: Some(true),
        pushed_authorization_request_endpoint: Some(format_smolstr!("{issuer}/par")),
        token_endpoint_auth_methods_supported: Some(vec![SmolStr::from("none")]),
        dpop_signing_alg_values_supported: Some(vec![SmolStr::from("ES256")]),
        authorization_response_iss_parameter_supported: Some(true),
        ..Default::default()
    }
}

impl OAuthResolver for MockClient {
    async fn resolve_oauth(
        &self,
        _input: &str,
    ) -> Result<
        (
            OAuthAuthorizationServerMetadata,
            Option<jacquard_common::types::did_doc::DidDocument>,
        ),
        ResolverError,
    > {
        let document =
            serde_json::from_value(did_document("did:plc:alice")).expect("valid DID document");
        Ok((server_metadata("https://issuer.test"), Some(document)))
    }

    async fn get_authorization_server_metadata(
        &self,
        issuer: &str,
    ) -> Result<OAuthAuthorizationServerMetadata, ResolverError> {
        Ok(server_metadata(issuer))
    }

    async fn get_resource_server_metadata(
        &self,
        _pds: &str,
    ) -> Result<OAuthAuthorizationServerMetadata, ResolverError> {
        Ok(server_metadata("https://issuer.test"))
    }
}

impl jacquard_oauth::dpop::DpopExt for MockClient {}

struct StartedFlow {
    client: MockClient,
    oauth: OAuthClient<MockClient, MemoryAuthStore>,
    state: SmolStr,
    expected_did: String,
}

async fn start_flow(label: &str) -> StartedFlow {
    let client = MockClient::default();
    client
        .push(
            Response::builder()
                .status(StatusCode::CREATED)
                .header(http::header::CONTENT_TYPE, "application/json")
                .body(
                    serde_json::to_vec(&serde_json::json!({
                        "request_uri": format!("urn:par:{label}"),
                        "expires_in": 60
                    }))
                    .unwrap(),
                )
                .unwrap(),
        )
        .await;
    let store = MemoryAuthStore::new();
    let expected_did = client
        .resolve_oauth("alice.bsky.social")
        .await
        .unwrap()
        .1
        .unwrap()
        .id
        .to_string();
    let oauth = OAuthClient::new_from_resolver(store, client.clone(), oauth_client_data().unwrap());
    let url = oauth
        .start_auth("alice.bsky.social", AuthorizeOptions::<SmolStr>::default())
        .await
        .unwrap();
    assert!(url.starts_with("https://issuer.test/authorize?"));
    assert!(url.contains("request_uri=urn%3Apar%3A"));

    let state = client.par_state().await;
    StartedFlow {
        client,
        oauth,
        state,
        expected_did,
    }
}

fn token_response(sub: &str, scope: &str) -> Response<Vec<u8>> {
    Response::builder()
        .status(StatusCode::OK)
        .header(http::header::CONTENT_TYPE, "application/json")
        .header("DPoP-Nonce", "next-nonce")
        .body(
            serde_json::to_vec(&serde_json::json!({
                "access_token": "test-access-secret",
                "token_type": "DPoP",
                "refresh_token": "test-refresh-secret",
                "sub": sub,
                "iss": "https://issuer.test",
                "aud": "https://pds.test",
                "scope": scope,
                "expires_in": 3600
            }))
            .unwrap(),
        )
        .unwrap()
}

fn dpop_nonce_challenge() -> Response<Vec<u8>> {
    Response::builder()
        .status(StatusCode::BAD_REQUEST)
        .header(http::header::CONTENT_TYPE, "application/json")
        .header("DPoP-Nonce", "required-nonce")
        .body(serde_json::to_vec(&serde_json::json!({"error": "use_dpop_nonce"})).unwrap())
        .unwrap()
}

#[tokio::test]
async fn par_pkce_and_dpop_are_present_in_the_successful_flow() {
    let flow = start_flow("success").await;
    flow.client.push(dpop_nonce_challenge()).await;
    flow.client
        .push(token_response(
            "did:plc:alice",
            "atproto repo:gay.dollspace.wildforge.device",
        ))
        .await;
    let session = flow
        .oauth
        .callback(CallbackParams {
            code: SmolStr::from("approved-code"),
            state: Some(flow.state.clone()),
            iss: Some(SmolStr::from("https://issuer.test")),
        })
        .await
        .unwrap();
    let (did, _) = session.session_info().await;
    assert_eq!(did.as_str(), "did:plc:alice");
    super::validate_oauth_subject(Some(&flow.expected_did), did.as_str()).unwrap();

    let requests = flow.client.take_requests().await;
    assert_eq!(requests.len(), 3);
    assert_eq!(requests[0].uri(), "https://issuer.test/par");
    let par = String::from_utf8_lossy(requests[0].body());
    assert!(par.contains("code_challenge="));
    assert!(par.contains("code_challenge_method=S256"));
    assert!(par.contains("state="));
    assert!(requests[0].headers().contains_key("dpop"));

    assert_eq!(requests[1].uri(), "https://issuer.test/token");
    let token = String::from_utf8_lossy(requests[1].body());
    assert!(token.contains("code_verifier="));
    assert!(requests[1].headers().contains_key("dpop"));
    assert_eq!(requests[2].uri(), "https://issuer.test/token");
    assert_eq!(requests[2].body(), requests[1].body());
    assert_ne!(
        requests[2].headers().get("dpop"),
        requests[1].headers().get("dpop")
    );
}

#[tokio::test]
async fn missing_mismatched_and_replayed_callback_state_is_rejected() {
    let flow = start_flow("state").await;
    let missing = match flow
        .oauth
        .callback(CallbackParams {
            code: SmolStr::from("code"),
            state: None,
            iss: Some(SmolStr::from("https://issuer.test")),
        })
        .await
    {
        Ok(_) => panic!("callback without state was accepted"),
        Err(error) => error,
    };
    assert!(missing.to_string().to_ascii_lowercase().contains("state"));

    let mismatch = match flow
        .oauth
        .callback(CallbackParams {
            code: SmolStr::from("code"),
            state: Some(flow.state.clone()),
            iss: Some(SmolStr::from("https://other-issuer.test")),
        })
        .await
    {
        Ok(_) => panic!("callback from a mismatched issuer was accepted"),
        Err(error) => error,
    };
    assert!(mismatch.to_string().to_ascii_lowercase().contains("issuer"));

    let replay = match flow
        .oauth
        .callback(CallbackParams {
            code: SmolStr::from("code"),
            state: Some(flow.state.clone()),
            iss: Some(SmolStr::from("https://issuer.test")),
        })
        .await
    {
        Ok(_) => panic!("consumed callback state was replayed"),
        Err(error) => error,
    };
    assert!(replay.to_string().to_ascii_lowercase().contains("state"));
    assert_eq!(flow.client.take_requests().await.len(), 1);
}

#[tokio::test(flavor = "multi_thread")]
async fn cancelled_browser_callback_is_not_misread_as_an_authorization_code() {
    let flow = start_flow("cancelled").await;
    let (address, handle) = one_shot_server(("127.0.0.1", 0)).await.unwrap();
    tokio::task::spawn_blocking(move || {
        let mut stream = std::net::TcpStream::connect(address).unwrap();
        stream
            .write_all(
                b"GET /oauth/callback?error=access_denied&error_description=cancelled HTTP/1.1\r\nHost: 127.0.0.1\r\nConnection: close\r\n\r\n",
            )
            .unwrap();
    })
    .await
    .unwrap();
    let config = LoopbackConfig {
        port: LoopbackPort::Ephemeral,
        open_browser: false,
        timeout_ms: 25,
        ..LoopbackConfig::default()
    };
    let error = match handle_localhost_callback(handle, &flow.oauth, &config).await {
        Ok(_) => panic!("cancelled browser callback created a session"),
        Err(error) => error,
    };
    assert!(error.to_string().to_ascii_lowercase().contains("timeout"));
    assert_eq!(flow.client.take_requests().await.len(), 1);
}

#[tokio::test]
async fn malformed_sub_and_denied_scope_are_rejected_without_leaking_tokens() {
    let malformed = start_flow("bad-sub").await;
    malformed
        .client
        .push(token_response(
            "not-a-did",
            "atproto repo:gay.dollspace.wildforge.device",
        ))
        .await;
    let error = match malformed
        .oauth
        .callback(CallbackParams {
            code: SmolStr::from("code"),
            state: Some(malformed.state.clone()),
            iss: Some(SmolStr::from("https://issuer.test")),
        })
        .await
    {
        Ok(_) => panic!("malformed token subject was accepted"),
        Err(error) => error,
    };
    let message = super::redact(&error.to_string());
    assert!(!message.contains("test-access-secret"));
    assert!(!message.contains("test-refresh-secret"));

    let wrong = start_flow("wrong-sub").await;
    wrong
        .client
        .push(token_response(
            "did:plc:mallory",
            "atproto repo:gay.dollspace.wildforge.device",
        ))
        .await;
    let wrong_session = wrong
        .oauth
        .callback(CallbackParams {
            code: SmolStr::from("code"),
            state: Some(wrong.state.clone()),
            iss: Some(SmolStr::from("https://issuer.test")),
        })
        .await
        .unwrap();
    let (wrong_did, _) = wrong_session.session_info().await;
    let wrong_error =
        super::validate_oauth_subject(Some(&wrong.expected_did), wrong_did.as_str()).unwrap_err();
    assert_eq!(wrong_error.kind(), std::io::ErrorKind::PermissionDenied);
    assert!(wrong_error.to_string().contains("subject"));

    let denied = start_flow("denied-scope").await;
    denied
        .client
        .push(
            Response::builder()
                .status(StatusCode::BAD_REQUEST)
                .header(http::header::CONTENT_TYPE, "application/json")
                .body(
                    serde_json::to_vec(&serde_json::json!({
                        "error": "invalid_scope",
                        "error_description": "binding permission denied"
                    }))
                    .unwrap(),
                )
                .unwrap(),
        )
        .await;
    let denied_error = match denied
        .oauth
        .callback(CallbackParams {
            code: SmolStr::from("code"),
            state: Some(denied.state.clone()),
            iss: Some(SmolStr::from("https://issuer.test")),
        })
        .await
    {
        Ok(_) => panic!("denied binding scope was accepted"),
        Err(error) => error,
    };
    assert!(
        denied_error
            .to_string()
            .to_ascii_lowercase()
            .contains("scope")
    );
}

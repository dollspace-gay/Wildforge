//! Real TLS handshakes for the guest certificate trust and key-possession contract.

use std::io::Cursor;
use std::sync::Arc;

use rustls::pki_types::{PrivatePkcs8KeyDer, ServerName};

use super::TrustAny;

#[derive(Debug)]
struct CertificateResolver(Arc<rustls::sign::CertifiedKey>);

impl rustls::server::ResolvesServerCert for CertificateResolver {
    fn resolve(
        &self,
        _: rustls::server::ClientHello<'_>,
    ) -> Option<Arc<rustls::sign::CertifiedKey>> {
        Some(Arc::clone(&self.0))
    }
}

fn handshake(
    version: &'static rustls::SupportedProtocolVersion,
    algorithm: &'static rcgen::SignatureAlgorithm,
    copied_certificate: bool,
) -> Result<(), rustls::Error> {
    let provider = Arc::new(rustls::crypto::ring::default_provider());
    let legitimate_key = rcgen::KeyPair::generate_for(algorithm).unwrap();
    let attacker_key = rcgen::KeyPair::generate_for(algorithm).unwrap();
    let certificate = rcgen::CertificateParams::new(vec!["wildforge".into()])
        .unwrap()
        .self_signed(&legitimate_key)
        .unwrap();
    let private_key = if copied_certificate {
        &attacker_key
    } else {
        &legitimate_key
    };
    let key = provider
        .key_provider
        .load_private_key(PrivatePkcs8KeyDer::from(private_key.serialize_der()).into())?;
    let pinned = certificate.der().clone();
    // A custom resolver intentionally bypasses rustls's configuration-time key
    // consistency check: the client must reject the attack during the handshake.
    let certified = rustls::sign::CertifiedKey::new(vec![pinned.clone()], key);
    assert_eq!(certified.keys_match().is_err(), copied_certificate);
    let server_config = rustls::ServerConfig::builder_with_provider(Arc::clone(&provider))
        .with_protocol_versions(&[version])?
        .with_no_client_auth()
        .with_cert_resolver(Arc::new(CertificateResolver(Arc::new(certified))));
    let builder =
        rustls::ClientConfig::builder_with_provider(provider).with_protocol_versions(&[version])?;
    let verifier = TrustAny {
        supported: builder.crypto_provider().signature_verification_algorithms,
    };
    let client_config = builder
        .dangerous()
        .with_custom_certificate_verifier(Arc::new(verifier))
        .with_no_client_auth();
    let mut client = rustls::ClientConnection::new(
        Arc::new(client_config),
        ServerName::try_from("wildforge").unwrap(),
    )?;
    let mut server = rustls::ServerConnection::new(Arc::new(server_config))?;
    for _ in 0..20 {
        let mut bytes = Vec::new();
        client.write_tls(&mut bytes).unwrap();
        server.read_tls(&mut Cursor::new(bytes)).unwrap();
        server.process_new_packets()?;
        let mut bytes = Vec::new();
        server.write_tls(&mut bytes).unwrap();
        client.read_tls(&mut Cursor::new(bytes)).unwrap();
        client.process_new_packets()?;
        if !client.is_handshaking() && !server.is_handshaking() {
            assert_eq!(client.protocol_version(), Some(version.version));
            assert_eq!(client.peer_certificates().unwrap()[0], pinned);
            return Ok(());
        }
    }
    panic!("TLS handshake did not finish within its flight bound");
}

#[test]
fn tls12_accepts_self_signed_host_with_matching_key() {
    handshake(
        &rustls::version::TLS12,
        &rcgen::PKCS_ECDSA_P256_SHA256,
        false,
    )
    .unwrap();
}

#[test]
fn tls13_accepts_self_signed_ecdsa_and_persistent_ed25519_host_keys() {
    for algorithm in [&rcgen::PKCS_ECDSA_P256_SHA256, &rcgen::PKCS_ED25519] {
        handshake(&rustls::version::TLS13, algorithm, false).unwrap();
    }
}

#[test]
fn tls12_rejects_copied_certificate_without_its_private_key() {
    let result = handshake(
        &rustls::version::TLS12,
        &rcgen::PKCS_ECDSA_P256_SHA256,
        true,
    );
    assert!(
        matches!(
            result,
            Err(rustls::Error::InvalidCertificate(
                rustls::CertificateError::BadSignature
            ))
        ),
        "copied public certificate with an unrelated key must fail signature verification: {result:?}"
    );
}

#[test]
fn tls13_rejects_copied_ecdsa_and_ed25519_certificates_without_their_private_keys() {
    for algorithm in [&rcgen::PKCS_ECDSA_P256_SHA256, &rcgen::PKCS_ED25519] {
        let result = handshake(&rustls::version::TLS13, algorithm, true);
        assert!(
            matches!(
                result,
                Err(rustls::Error::InvalidCertificate(
                    rustls::CertificateError::BadSignature
                ))
            ),
            "copied public certificate with an unrelated key must fail signature verification: {result:?}"
        );
    }
}

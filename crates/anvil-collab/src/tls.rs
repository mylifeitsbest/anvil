//! TLS 1.3 с привязкой к отпечатку сертификата.
//!
//! Relay работает с самоподписанным сертификатом и сообщает его SHA-256. Отпечаток входит в
//! ссылку-приглашение, и клиент принимает только сертификат с этим отпечатком — подменить
//! сервер посередине нельзя, а покупать сертификат или настраивать домен не нужно.

use std::sync::Arc;

use rustls::client::danger::{HandshakeSignatureValid, ServerCertVerified, ServerCertVerifier};
use rustls::crypto::{CryptoProvider, verify_tls12_signature, verify_tls13_signature};
use rustls::pki_types::{CertificateDer, PrivateKeyDer, PrivatePkcs8KeyDer, ServerName, UnixTime};
use rustls::{CertificateError, ClientConfig, DigitallySignedStruct, ServerConfig, SignatureScheme};

pub type Fingerprint = [u8; 32];

/// SHA-256 сертификата в DER.
pub fn fingerprint(der: &[u8]) -> Fingerprint {
    let d = ring::digest::digest(&ring::digest::SHA256, der);
    let mut out = [0u8; 32];
    out.copy_from_slice(d.as_ref());
    out
}

pub fn fp_hex(fp: &Fingerprint) -> String {
    crate::hex(fp)
}

/// Отпечаток из шестнадцатеричной строки (двоеточия допускаются).
pub fn parse_fp(s: &str) -> Option<Fingerprint> {
    let v = crate::unhex(s.trim())?;
    v.try_into().ok()
}

fn provider() -> Arc<CryptoProvider> {
    Arc::new(rustls::crypto::ring::default_provider())
}

#[derive(Debug)]
struct Pinned {
    fp: Fingerprint,
    provider: Arc<CryptoProvider>,
}

impl ServerCertVerifier for Pinned {
    fn verify_server_cert(
        &self,
        end_entity: &CertificateDer<'_>,
        _intermediates: &[CertificateDer<'_>],
        _server_name: &ServerName<'_>,
        _ocsp: &[u8],
        _now: UnixTime,
    ) -> Result<ServerCertVerified, rustls::Error> {
        let got = fingerprint(end_entity.as_ref());
        let same = got.iter().zip(self.fp.iter()).fold(0u8, |a, (x, y)| a | (x ^ y)) == 0;
        if same { Ok(ServerCertVerified::assertion()) } else { Err(rustls::Error::InvalidCertificate(CertificateError::ApplicationVerificationFailure)) }
    }

    fn verify_tls12_signature(&self, message: &[u8], cert: &CertificateDer<'_>, dss: &DigitallySignedStruct) -> Result<HandshakeSignatureValid, rustls::Error> {
        verify_tls12_signature(message, cert, dss, &self.provider.signature_verification_algorithms)
    }

    fn verify_tls13_signature(&self, message: &[u8], cert: &CertificateDer<'_>, dss: &DigitallySignedStruct) -> Result<HandshakeSignatureValid, rustls::Error> {
        verify_tls13_signature(message, cert, dss, &self.provider.signature_verification_algorithms)
    }

    fn supported_verify_schemes(&self) -> Vec<SignatureScheme> {
        self.provider.signature_verification_algorithms.supported_schemes()
    }
}

/// Настройки клиента: TLS 1.3, сервер проверяется по отпечатку.
///
/// Имя сервера (SNI) не отправляется: оно не нужно — сервер сверяется по отпечатку, — а часть
/// сетевых фильтров (DPI) рвёт TLS-соединения к «незнакомым» адресам с SNI, но пропускает без него.
pub fn client_config(fp: Fingerprint) -> Result<Arc<ClientConfig>, rustls::Error> {
    let provider = provider();
    let mut cfg = ClientConfig::builder_with_provider(provider.clone())
        .with_protocol_versions(&[&rustls::version::TLS13])?
        .dangerous()
        .with_custom_certificate_verifier(Arc::new(Pinned { fp, provider }))
        .with_no_client_auth();
    cfg.enable_sni = false;
    Ok(Arc::new(cfg))
}

/// Настройки сервера по сертификату и ключу PKCS#8 (DER).
pub fn server_config(cert: &[u8], key: &[u8]) -> Result<Arc<ServerConfig>, rustls::Error> {
    let cfg = ServerConfig::builder_with_provider(provider())
        .with_protocol_versions(&[&rustls::version::TLS13])?
        .with_no_client_auth()
        .with_single_cert(vec![CertificateDer::from(cert.to_vec())], PrivateKeyDer::Pkcs8(PrivatePkcs8KeyDer::from(key.to_vec())))?;
    Ok(Arc::new(cfg))
}

/// Имя сервера для рукопожатия (в сеть не уходит — SNI выключен). Проверяется отпечаток, а не имя.
pub fn server_name() -> Result<ServerName<'static>, rustls::pki_types::InvalidDnsNameError> {
    ServerName::try_from("anvil-relay")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fingerprint_hex() {
        let fp = fingerprint(b"hello");
        assert_eq!(fp_hex(&fp), "2cf24dba5fb0a30e26e83b2ac5b9e29e1b161e5c1fa7425e73043362938b9824");
        assert_eq!(parse_fp(&fp_hex(&fp)), Some(fp));
        assert_eq!(parse_fp("abcd"), None);
    }

    #[test]
    fn server_name_is_valid() {
        assert!(matches!(server_name(), Ok(ServerName::DnsName(_))));
    }

    #[test]
    fn client_sends_no_sni() -> Result<(), rustls::Error> {
        assert!(!client_config([0; 32])?.enable_sni);
        Ok(())
    }
}

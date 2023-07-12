// Copyright (c) 2023 The MobileCoin Foundation

//! The full set of evidence needed for attesting a quote

use crate::{
    choice_to_status_message, Accessor, Advisories, CertificateChainVerifier,
    CertificateChainVerifierError, Error, QeIdentity, SignedQeIdentity, SignedTcbInfo,
    SignedTcbInfoVerifier, TcbInfo, TrustedIdentity, VerificationMessage, VerificationOutput,
    Verifier, MESSAGE_INDENT,
};
use alloc::vec::Vec;
use core::fmt::Formatter;
use der::{DateTime, DecodePem};
use mc_sgx_core_types::{
    Attributes, ConfigId, ConfigSvn, CpuSvn, ExtendedProductId, FamilyId, IsvProductId, IsvSvn,
    MiscellaneousSelect, MrEnclave, MrSigner, ReportData,
};
use mc_sgx_dcap_types::{CertificationData, Collateral, Quote3, TcbInfo as QuoteTcbInfo};
use p256::ecdsa::VerifyingKey;
use x509_cert::crl::CertificateList;
use x509_cert::Certificate;

/// The full set of evidence needed for verifying a quote
///
/// A wrapping container for a `Quote3` and a `Collateral`. This can be used
/// with the majority of the `Verifier` implementations from this crate.
/// This allows one to compose one verifier and use an [`Evidence`] instance in
/// the `verify()` method.
///
/// Importantly this will derive the [`Advisories`] related to the provided
/// `quote` and `collateral`, so that one can verify the allowed advisories.
#[derive(Debug, Clone)]
pub struct Evidence<Q> {
    quote: Quote3<Q>,
    signed_tcb_info: SignedTcbInfo,
    signed_qe_identity: SignedQeIdentity,
    _qe_identity: QeIdentity,
    advisories: Advisories,
    collateral: Collateral,
}

impl<Q: AsRef<[u8]>> Evidence<Q> {
    /// Create a new instance
    pub fn new(quote: Quote3<Q>, collateral: Collateral) -> Result<Self, Error> {
        // We perform any fallible conversions now to keep the verification focused on the values
        // and not the types/format.
        let signed_qe_identity = SignedQeIdentity::try_from(collateral.qe_identity())?;
        let qe_identity = QeIdentity::try_from(&signed_qe_identity)?;

        let signed_tcb_info = SignedTcbInfo::try_from(collateral.tcb_info())?;
        let quote_tcb_info = tcb_info_try_from_quote(&quote)?;
        let tcb_info = TcbInfo::try_from(&signed_tcb_info)?;
        let advisories = tcb_info.advisories(&quote_tcb_info)?;
        Ok(Self {
            quote,
            signed_tcb_info,
            signed_qe_identity,
            _qe_identity: qe_identity,
            advisories,
            collateral,
        })
    }
}

impl From<Evidence<&[u8]>> for Evidence<Vec<u8>> {
    fn from(value: Evidence<&[u8]>) -> Self {
        Self {
            // See https://github.com/mobilecoinfoundation/sgx/issues/357 to make the quote
            // conversion more ergonomic
            quote: value
                .quote
                .as_ref()
                .to_vec()
                .try_into()
                .expect("Quote should already be valid"),
            signed_tcb_info: value.signed_tcb_info,
            signed_qe_identity: value.signed_qe_identity,
            _qe_identity: value._qe_identity,
            advisories: value.advisories,
            collateral: value.collateral,
        }
    }
}

impl<Q> Accessor<SignedQeIdentity> for Evidence<Q> {
    fn get(&self) -> SignedQeIdentity {
        self.signed_qe_identity.clone()
    }
}

impl<Q> Accessor<SignedTcbInfo> for Evidence<Q> {
    fn get(&self) -> SignedTcbInfo {
        self.signed_tcb_info.clone()
    }
}

impl<Q: Clone> Accessor<Quote3<Q>> for Evidence<Q> {
    fn get(&self) -> Quote3<Q> {
        self.quote.clone()
    }
}

impl<Q> Accessor<Advisories> for Evidence<Q> {
    fn get(&self) -> Advisories {
        self.advisories.clone()
    }
}

// Get the certificate chain from the quote's certification data. Table 9 in appendix A of
// <https://download.01.org/intel-sgx/latest/dcap-latest/linux/docs/Intel_SGX_ECDSA_QuoteLibReference_DCAP_API.pdf#%5B%7B%22num%22%3A77%2C%22gen%22%3A0%7D%2C%7B%22name%22%3A%22XYZ%22%7D%2C120%2C282%2C0%5D>
fn certificate_chain_try_from_quote<Q: AsRef<[u8]>>(
    quote: &Quote3<Q>,
) -> Result<Vec<Certificate>, Error> {
    let signature_data = quote.signature_data();
    let certification_data = signature_data.certification_data();
    let CertificationData::PckCertificateChain(pem_chain) = certification_data else {
        return Err(Error::UnsupportedQuoteCertificationData);
    };
    Ok(pem_chain
        .into_iter()
        .map(Certificate::from_pem)
        .collect::<Result<Vec<_>, _>>()?)
}

// TODO think this should go in tcb.rs of `mc-sgx-dcap-types`
fn tcb_info_try_from_quote<Q: AsRef<[u8]>>(quote: &Quote3<Q>) -> Result<QuoteTcbInfo, Error> {
    let chain = certificate_chain_try_from_quote(quote)?;
    let leaf_cert = chain
        .first()
        .ok_or(Error::UnsupportedQuoteCertificationData)?;
    Ok(QuoteTcbInfo::try_from(leaf_cert)?)
}

/// Macro to generate boilerplate for implementing [`Accessor`] for a field of
/// the application enclave [`ReportBody`] in the evidence's [`Quote3`]
///
/// # Arguments
/// * `field_type` - The type of the field in `ReportBody` to be accessed
/// * `accessor_method` - The method on `ReportBody` that returns the field
macro_rules! quote_application_report_body_field_accessor {
    ($($field_type:ty, $accessor_method:ident;)*) => {$(
        impl<Q: AsRef<[u8]>> Accessor<$field_type> for Evidence<Q> {
            fn get(&self) -> $field_type {
                self.quote.app_report_body().$accessor_method()
            }
        }
    )*}
}

quote_application_report_body_field_accessor! {
    Attributes, attributes;
    ConfigId, config_id;
    ConfigSvn, config_svn;
    CpuSvn, cpu_svn;
    ExtendedProductId, isv_extended_product_id;
    FamilyId, isv_family_id;
    IsvProductId, isv_product_id;
    IsvSvn, isv_svn;
    MiscellaneousSelect, miscellaneous_select;
    MrEnclave, mr_enclave;
    MrSigner, mr_signer;
    ReportData, report_data;
}

/// Verifier for evidence
///
/// This will perform most of the verification to be done on [`Evidence`] this includes:
/// - verifying the certificate chains
/// - verifying the QE identity (TODO)
/// - verifying the TCB info
/// - verifying the [`TrustedIdentity`] of the application enclave (TODO)
/// - verifying the signature of the Quote (TODO)
#[derive(Debug)]
pub struct EvidenceVerifier<'a, C> {
    certificate_verifier: &'a C,
    _trusted_identities: Vec<TrustedIdentity>,
    time: DateTime,
}

impl<'a, C> EvidenceVerifier<'a, C>
where
    C: CertificateChainVerifier,
{
    /// Create a new instance
    ///
    /// # Arguments
    /// * `certificate_verifier` - The verifier to use for verifying the certificate chains. The
    ///   certificate chains all use a common trust root and the verifier is responsible for
    ///   knowing the trust root and verifying the chains up to that root.
    /// * `trusted_identities` - The trusted identities to use for verifying the evidence. Only one
    ///   of the identities needs to match to succeed for the identity verification portion.
    /// * `time` - The time to use for verifying the evidence. In particular the TCB Info and QE
    ///   identity have expiry times that need to be verified. Note: that the `certificate_verifier`
    ///   will also be passed this time.
    pub fn new<I, ID>(certificate_verifier: &'a C, trusted_identities: I, time: DateTime) -> Self
    where
        I: IntoIterator<Item = ID>,
        ID: Into<TrustedIdentity>,
    {
        Self {
            certificate_verifier,
            _trusted_identities: trusted_identities.into_iter().map(Into::into).collect(),
            time,
        }
    }

    // Assumes that `chain` is ordered such that the leaf is the first element and root is the last.
    //
    // This order matches that documented at
    // <https://api.portal.trustedservices.intel.com/documentation#pcs-tcb-info-v4>
    //
    //      all certificates in the chain, appended to each other in the following order:
    //      <Signing Certificate><Root CA Certificate>)
    //
    // and in
    // <https://download.01.org/intel-sgx/latest/dcap-latest/linux/docs/Intel_SGX_ECDSA_QuoteLibReference_DCAP_API.pdf#%5B%7B%22num%22%3A77%2C%22gen%22%3A0%7D%2C%7B%22name%22%3A%22XYZ%22%7D%2C120%2C282%2C0%5D>
    //
    //      5: Concatenated PCK Cert Chain (PEM formatted).
    //      PCK Leaf Cert||Intermediate CA Cert||Root CA Cert
    //
    fn verify_certificate_chain<'c, 'd>(
        &self,
        chain: &[Certificate],
        crls: impl IntoIterator<Item = &'d CertificateList>,
    ) -> (
        Option<VerifyingKey>,
        VerificationOutput<Option<CertificateChainVerifierError>>,
    ) {
        let result = self
            .certificate_verifier
            .verify_certificate_chain(chain, crls, self.time);
        let is_success = result.is_ok() as u8;

        // Using the default key will result in the user seeing "Error verifying the signature" for
        // the signed data. So we try to get the key from the certificate chain, even if the
        // verification failed. This handles the most likely failure case of an expired
        // certificate, whose key is still the key that signed the data of interest.
        let key = chain.first().and_then(key_from_certificate);

        (
            key,
            VerificationOutput::new(result.err(), is_success.into()),
        )
    }

    fn verify_tcb_signing_chain(
        &self,
        collateral: &Collateral,
    ) -> (
        Option<VerifyingKey>,
        VerificationOutput<Option<CertificateChainVerifierError>>,
    ) {
        let chain = collateral.tcb_issuer_chain();
        let crls = [collateral.root_ca_crl()];
        self.verify_certificate_chain(chain, crls)
    }

    fn verify_qe_identity_signing_chain(
        &self,
        collateral: &Collateral,
    ) -> (
        Option<VerifyingKey>,
        VerificationOutput<Option<CertificateChainVerifierError>>,
    ) {
        let chain = collateral.qe_identity_issuer_chain();
        let crls = [collateral.root_ca_crl()];
        self.verify_certificate_chain(chain, crls)
    }

    fn verify_quote_signing_chain<Q: AsRef<[u8]>>(
        &self,
        quote: &Quote3<Q>,
        collateral: &Collateral,
    ) -> (
        Option<VerifyingKey>,
        VerificationOutput<Option<CertificateChainVerifierError>>,
    ) {
        let crls = [collateral.root_ca_crl(), collateral.pck_crl()];
        // The Quote's chain is not in the collateral. It is in the quote itself.
        // As documented in table 9 of appendix A,
        // <https://download.01.org/intel-sgx/latest/dcap-latest/linux/docs/Intel_SGX_ECDSA_QuoteLibReference_DCAP_API.pdf#%5B%7B%22num%22%3A77%2C%22gen%22%3A0%7D%2C%7B%22name%22%3A%22XYZ%22%7D%2C120%2C282%2C0%5D>
        // the certificate chain is at the end of the quote bytes for quotes with type `5`
        // certification data.
        match certificate_chain_try_from_quote(quote) {
            Ok(chain) => self.verify_certificate_chain(&chain, crls),
            Err(_) => {
                let is_success = 0u8;
                (
                    None,
                    VerificationOutput::new(
                        Some(CertificateChainVerifierError::GeneralCertificateError),
                        is_success.into(),
                    ),
                )
            }
        }
    }
}

fn key_from_certificate(cert: &Certificate) -> Option<VerifyingKey> {
    let key_bytes = cert
        .tbs_certificate
        .subject_public_key_info
        .subject_public_key
        .as_bytes()?;
    VerifyingKey::from_sec1_bytes(key_bytes).ok()
}

impl<'a, C: CertificateChainVerifier, E: Accessor<Evidence<Vec<u8>>>> Verifier<E>
    for EvidenceVerifier<'a, C>
{
    type Value = EvidenceValue;

    fn verify(&self, evidence: &E) -> VerificationOutput<Self::Value> {
        let evidence = evidence.get();
        let collateral = &evidence.collateral;
        let quote = &evidence.quote;

        let (tcb_key, tcb_chain_verification) = self.verify_tcb_signing_chain(collateral);
        let (_, qe_chain_verification) = self.verify_qe_identity_signing_chain(collateral);
        let (_, quote_chain_verification) = self.verify_quote_signing_chain(quote, collateral);

        let tcb_info_verifier = SignedTcbInfoVerifier::new(tcb_key, self.time);
        let tcb_info_verification = tcb_info_verifier.verify(&evidence);

        let evidence_value = EvidenceValue {
            tcb_signing_key: tcb_chain_verification,
            qe_identity_signing_key: qe_chain_verification,
            quote_signing_key: quote_chain_verification,
            tcb_info: (tcb_info_verifier, tcb_info_verification),
        };

        let is_success = evidence_value.tcb_signing_key.is_success()
            & evidence_value.qe_identity_signing_key.is_success()
            & evidence_value.quote_signing_key.is_success()
            & evidence_value.tcb_info.1.is_success();

        VerificationOutput::new(evidence_value, is_success)
    }
}

#[derive(Debug)]
pub struct EvidenceValue {
    tcb_signing_key: VerificationOutput<Option<CertificateChainVerifierError>>,
    qe_identity_signing_key: VerificationOutput<Option<CertificateChainVerifierError>>,
    quote_signing_key: VerificationOutput<Option<CertificateChainVerifierError>>,
    tcb_info: (SignedTcbInfoVerifier, VerificationOutput<Option<Error>>),
}

fn fmt_chain_verification_result_padded(
    f: &mut Formatter<'_>,
    pad: usize,
    name: &str,
    result: &VerificationOutput<Option<CertificateChainVerifierError>>,
) -> core::fmt::Result {
    let is_success = result.is_success();
    let status = choice_to_status_message(is_success);
    write!(f, "{:pad$}{status} The {name} issuer chain ", "")?;

    if is_success.into() {
        write!(f, "was verified.")
    } else {
        let error = result
            .value()
            .as_ref()
            .expect("Should have an error if not successful");
        write!(f, "could not be verified: {error}")
    }
}

impl<'a, C> VerificationMessage<EvidenceValue> for EvidenceVerifier<'a, C>
where
    C: CertificateChainVerifier,
{
    fn fmt_padded(
        &self,
        f: &mut Formatter<'_>,
        pad: usize,
        output: &VerificationOutput<EvidenceValue>,
    ) -> core::fmt::Result {
        let status = choice_to_status_message(output.is_success());

        write!(f, "{:pad$}{status} all of the following must be true:", "")?;
        let pad = pad + MESSAGE_INDENT;
        writeln!(f)?;
        fmt_chain_verification_result_padded(f, pad, "TCB", &output.value.tcb_signing_key)?;
        writeln!(f)?;
        fmt_chain_verification_result_padded(
            f,
            pad,
            "QE identity",
            &output.value.qe_identity_signing_key,
        )?;
        writeln!(f)?;
        fmt_chain_verification_result_padded(f, pad, "Quote", &output.value.quote_signing_key)?;
        writeln!(f)?;
        let (tcb_info_verifier, tcb_info_verification) = &output.value.tcb_info;
        tcb_info_verifier.fmt_padded(f, pad, tcb_info_verification)
    }
}

#[cfg(test)]
mod test {
    use super::*;
    use crate::{VerificationTreeDisplay, Verifier};
    use alloc::format;
    use alloc::string::{String, ToString};
    use assert_matches::assert_matches;
    use core::mem;
    use mc_sgx_dcap_sys_types::{sgx_ql_ecdsa_sig_data_t, sgx_ql_qve_collateral_t, sgx_quote3_t};

    const TCB_INFO_JSON: &str = include_str!("../data/tests/fmspc_00906ED50000_2023_05_10.json");
    const QE_IDENTITY_JSON: &str = include_str!("../data/tests/qe_identity.json");

    fn collateral(tcb_info: &str, qe_identity: &str) -> Collateral {
        let mut sgx_collateral = sgx_ql_qve_collateral_t::default();

        // SAFETY: Version is a union which is inherently unsafe
        #[allow(unsafe_code)]
        let version = unsafe { sgx_collateral.__bindgen_anon_1.__bindgen_anon_1.as_mut() };
        version.major_version = 3;
        version.minor_version = 1;

        let pck_issuer_cert = include_str!("../data/tests/processor_ca.pem");
        let root_cert = include_str!("../data/tests/root_ca.pem");
        let mut pck_crl_chain = [pck_issuer_cert, root_cert].join("\n").as_bytes().to_vec();
        pck_crl_chain.push(0);
        sgx_collateral.pck_crl_issuer_chain = pck_crl_chain.as_ptr() as _;
        sgx_collateral.pck_crl_issuer_chain_size = pck_crl_chain.len() as u32;

        let mut root_crl = include_bytes!("../data/tests/root_crl.der").to_vec();
        root_crl.push(0);
        sgx_collateral.root_ca_crl = root_crl.as_ptr() as _;
        sgx_collateral.root_ca_crl_size = root_crl.len() as u32;

        let mut pck_crl = include_bytes!("../data/tests/processor_crl.der").to_vec();
        pck_crl.push(0);
        sgx_collateral.pck_crl = pck_crl.as_ptr() as _;
        sgx_collateral.pck_crl_size = pck_crl.len() as u32;

        let tcb_cert = include_str!("../data/tests/tcb_signer.pem");
        let mut tcb_chain = [tcb_cert, root_cert].join("\n").as_bytes().to_vec();
        tcb_chain.push(0);
        sgx_collateral.tcb_info_issuer_chain = tcb_chain.as_ptr() as _;
        sgx_collateral.tcb_info_issuer_chain_size = tcb_chain.len() as u32;

        sgx_collateral.tcb_info = tcb_info.as_ptr() as _;
        sgx_collateral.tcb_info_size = tcb_info.len() as u32;

        // For live data the QE identity uses the same chain as the TCB info
        sgx_collateral.qe_identity_issuer_chain = tcb_chain.as_ptr() as _;
        sgx_collateral.qe_identity_issuer_chain_size = tcb_chain.len() as u32;

        sgx_collateral.qe_identity = qe_identity.as_ptr() as _;
        sgx_collateral.qe_identity_size = qe_identity.len() as u32;

        Collateral::try_from(&sgx_collateral).expect("Failed to parse collateral")
    }

    #[test]
    fn evidence_fails_due_to_wrong_quote_certification_data() {
        let mut quote_bytes = include_bytes!("../data/tests/hw_quote.dat").to_vec();

        // The offset logic is based on
        // <https://download.01.org/intel-sgx/latest/dcap-latest/linux/docs/Intel_SGX_ECDSA_QuoteLibReference_DCAP_API.pdf#%5B%7B%22num%22%3A72%2C%22gen%22%3A0%7D%2C%7B%22name%22%3A%22XYZ%22%7D%2C69%2C356%2C0%5D>
        let auth_data_offset =
            mem::size_of::<sgx_quote3_t>() + mem::size_of::<sgx_ql_ecdsa_sig_data_t>();
        let auth_data_size = u16::from_le_bytes([
            quote_bytes[auth_data_offset],
            quote_bytes[auth_data_offset + 1],
        ]) as usize;

        // "2" is for the u16 for reading in the auth data size
        let cert_data_type_offset = auth_data_offset + auth_data_size + 2;

        // Not all types are supported so we set to 1
        // (PPID in plain text, CPUSVN and PCESVN)
        quote_bytes[cert_data_type_offset] = 1;

        let quote = Quote3::try_from(quote_bytes).expect("Failed to parse quote");

        let collateral = collateral(TCB_INFO_JSON, QE_IDENTITY_JSON);

        assert_matches!(
            Evidence::new(quote, collateral),
            Err(Error::UnsupportedQuoteCertificationData)
        );
    }

    #[test]
    fn evidence_fails_due_inability_get_advisories() {
        let quote_bytes = include_bytes!("../data/tests/hw_quote.dat");
        let quote = Quote3::try_from(quote_bytes.as_ref()).expect("Failed to parse quote");

        let tcb_json = include_str!("../data/tests/example_tcb.json");
        let collateral = collateral(tcb_json, QE_IDENTITY_JSON);

        assert_matches!(Evidence::new(quote, collateral), Err(Error::FmspcMismatch));
    }

    #[test]
    fn evidence_fails_due_inability_get_underlying_tcb_info() {
        let quote_bytes = include_bytes!("../data/tests/hw_quote.dat");
        let quote = Quote3::try_from(quote_bytes.as_ref()).expect("Failed to parse quote");

        let tcb_json = TCB_INFO_JSON;
        let bad_tcb_json = tcb_json.replace("SWHardeningNeeded", "NotGonnaHappen");
        let collateral = collateral(bad_tcb_json.as_str(), QE_IDENTITY_JSON);

        assert_matches!(Evidence::new(quote, collateral), Err(Error::Serde(_)));
    }

    #[test]
    fn evidence_fails_due_inability_get_underlying_signed_qe_identity() {
        let quote_bytes = include_bytes!("../data/tests/hw_quote.dat");
        let quote = Quote3::try_from(quote_bytes.as_ref()).expect("Failed to parse quote");

        let qe_json = QE_IDENTITY_JSON;

        // Modifies an outer JSON field, i.e. only the Signed QE Identity
        let bad_qe_json = qe_json.replace("enclaveIdentity", "NotGonnaHappen");

        let collateral = collateral(TCB_INFO_JSON, bad_qe_json.as_str());

        assert_matches!(Evidence::new(quote, collateral), Err(Error::Serde(_)));
    }

    #[test]
    fn evidence_fails_due_inability_get_underlying_qe_identity() {
        let quote_bytes = include_bytes!("../data/tests/hw_quote.dat");
        let quote = Quote3::try_from(quote_bytes.as_ref()).expect("Failed to parse quote");

        let qe_json = QE_IDENTITY_JSON;

        // Modifies a JSON field inside of the `enclaveIdentity`
        let bad_qe_json = qe_json.replace("UpToDate", "NotGonnaHappen");

        let collateral = collateral(TCB_INFO_JSON, bad_qe_json.as_str());

        assert_matches!(Evidence::new(quote, collateral), Err(Error::Serde(_)));
    }

    struct TestDoubleChainVerifier {
        failed_certificate_subject: String,
        error: CertificateChainVerifierError,
    }

    impl Default for TestDoubleChainVerifier {
        fn default() -> Self {
            Self {
                failed_certificate_subject: String::new(),
                error: CertificateChainVerifierError::GeneralCertificateError,
            }
        }
    }

    impl TestDoubleChainVerifier {
        // Cause certificate chain verification to fail at the subject certificate with `error`
        fn fail_at_certificate(subject: &str, error: CertificateChainVerifierError) -> Self {
            Self {
                failed_certificate_subject: String::from(subject),
                error,
            }
        }

        fn try_forced_failure(
            &self,
            subject_names: &[String],
        ) -> Result<(), CertificateChainVerifierError> {
            if subject_names.contains(&self.failed_certificate_subject) {
                Err(self.error.clone())
            } else {
                Ok(())
            }
        }

        fn verify_all_crls_present(subject_names: &[String], crls: &[&CertificateList]) {
            let crl_subject_names = crls
                .iter()
                .map(|crl| crl.tbs_cert_list.issuer.to_string())
                .collect::<Vec<_>>();

            // We take advantage that certificate chains are ordered from the subject to the root
            // and skip the leaf since it won't have a CRL.
            if !subject_names[1..]
                .into_iter()
                .all(|name| crl_subject_names.contains(name))
            {
                panic!("Missing a CRL for the certificate chain");
            }
        }

        fn verify_crl_time_is_valid(&self, crl: &CertificateList, time: DateTime) {
            let start_time = crl.tbs_cert_list.this_update.to_unix_duration();
            let end_time = crl
                .tbs_cert_list
                .next_update
                .expect("No next update time")
                .to_unix_duration();
            let time = time.unix_duration();
            if !(start_time <= time && time < end_time) {
                panic!("Time not valid");
            }
        }
    }

    impl CertificateChainVerifier for TestDoubleChainVerifier {
        // This is a test verifier, it does not verify the certificate chains, but instead verifies
        // that the `CertificateChainVerifier` is correctly used by the `EvidenceVerifier`
        //
        // If constructed with the `fail_at_certificate` method, it will fail if the subject of a
        // certificate in the `certificate_chain` matches.
        //
        // For test verification, the `time` provided should be within range of the first CRL.
        // Normally `time` would be system time.
        fn verify_certificate_chain<'a, 'b>(
            &self,
            certificate_chain: impl IntoIterator<Item = &'a Certificate>,
            crls: impl IntoIterator<Item = &'b CertificateList>,
            time: DateTime,
        ) -> Result<(), CertificateChainVerifierError> {
            let certificate_chain = certificate_chain.into_iter().collect::<Vec<_>>();
            let subject_names = certificate_chain
                .iter()
                .map(|cert| cert.tbs_certificate.subject.to_string())
                .collect::<Vec<_>>();

            self.try_forced_failure(&subject_names)?;

            let crls = crls.into_iter().collect::<Vec<_>>();
            Self::verify_all_crls_present(&subject_names, &crls);

            // Loose assurance that time was passed through
            self.verify_crl_time_is_valid(&crls[0], time);

            Ok(())
        }
    }

    #[test]
    fn evidence_verifier_succeeds() {
        let time = "2023-06-08T15:55:15Z"
            .parse::<DateTime>()
            .expect("Failed to parse time");

        let certificate_verifier = TestDoubleChainVerifier::default();

        let verifier =
            EvidenceVerifier::new(&certificate_verifier, [] as [TrustedIdentity; 0], time);

        let quote_bytes = include_bytes!("../data/tests/hw_quote.dat");
        let quote = Quote3::try_from(quote_bytes.as_ref()).expect("Failed to parse quote");
        let collateral = collateral(TCB_INFO_JSON, QE_IDENTITY_JSON);
        let evidence: Evidence<Vec<u8>> = Evidence::new(quote, collateral)
            .expect("Failed to create evidence")
            .into();

        let verification = verifier.verify(&evidence);

        assert_eq!(verification.is_success().unwrap_u8(), 1);

        let displayable = VerificationTreeDisplay::new(&verifier, verification);
        let expected = r#"
            - [x] all of the following must be true:
              - [x] The TCB issuer chain was verified.
              - [x] The QE identity issuer chain was verified.
              - [x] The Quote issuer chain was verified.
              - [x] The TCB info was verified for the provided key"#;
        assert_eq!(format!("\n{displayable}"), textwrap::dedent(expected));
    }

    #[test]
    fn evidence_verifier_fails_for_expired_quote_certificate() {
        let time = "2023-06-08T15:55:15Z"
            .parse::<DateTime>()
            .expect("Failed to parse time");

        let certificate_verifier = TestDoubleChainVerifier::fail_at_certificate("CN=Intel SGX PCK Certificate,O=Intel Corporation,L=Santa Clara,STATEORPROVINCENAME=CA,C=US", CertificateChainVerifierError::CertificateExpired);

        let verifier =
            EvidenceVerifier::new(&certificate_verifier, [] as [TrustedIdentity; 0], time);

        let quote_bytes = include_bytes!("../data/tests/hw_quote.dat");
        let quote = Quote3::try_from(quote_bytes.to_vec()).expect("Failed to parse quote");
        let collateral = collateral(TCB_INFO_JSON, QE_IDENTITY_JSON);
        let evidence = Evidence::new(quote, collateral).expect("Failed to create evidence");

        let verification = verifier.verify(&evidence);

        assert_eq!(verification.is_success().unwrap_u8(), 0);

        let displayable = VerificationTreeDisplay::new(&verifier, verification);
        let expected = r#"
            - [ ] all of the following must be true:
              - [x] The TCB issuer chain was verified.
              - [x] The QE identity issuer chain was verified.
              - [ ] The Quote issuer chain could not be verified: X509 certificate has expired
              - [x] The TCB info was verified for the provided key"#;
        assert_eq!(format!("\n{displayable}"), textwrap::dedent(expected));
    }

    #[test]
    fn evidence_verifier_fails_for_tcb_certificate_revoked() {
        let time = "2023-06-08T15:55:15Z"
            .parse::<DateTime>()
            .expect("Failed to parse time");

        let certificate_verifier = TestDoubleChainVerifier::fail_at_certificate("CN=Intel SGX TCB Signing,O=Intel Corporation,L=Santa Clara,STATEORPROVINCENAME=CA,C=US",
         CertificateChainVerifierError::CertificateRevoked);

        let verifier =
            EvidenceVerifier::new(&certificate_verifier, [] as [TrustedIdentity; 0], time);

        let quote_bytes = include_bytes!("../data/tests/hw_quote.dat");
        let quote = Quote3::try_from(quote_bytes.to_vec()).expect("Failed to parse quote");
        let collateral = collateral(TCB_INFO_JSON, QE_IDENTITY_JSON);
        let evidence = Evidence::new(quote, collateral).expect("Failed to create evidence");

        let verification = verifier.verify(&evidence);

        assert_eq!(verification.is_success().unwrap_u8(), 0);

        let displayable = VerificationTreeDisplay::new(&verifier, verification);
        // Note that the TCB and QE identity happen to use the same certificate, so the QE identity
        // will also fail.
        let expected = r#"
            - [ ] all of the following must be true:
              - [ ] The TCB issuer chain could not be verified: X509 certificate has been revoked
              - [ ] The QE identity issuer chain could not be verified: X509 certificate has been revoked
              - [x] The Quote issuer chain was verified.
              - [x] The TCB info was verified for the provided key"#;
        assert_eq!(format!("\n{displayable}"), textwrap::dedent(expected));
    }

    #[test]
    fn evidence_verifier_fails_for_expired_tcb_info() {
        let time = "2023-06-14T15:55:15Z"
            .parse::<DateTime>()
            .expect("Failed to parse time");

        let certificate_verifier = TestDoubleChainVerifier::default();

        let verifier =
            EvidenceVerifier::new(&certificate_verifier, [] as [TrustedIdentity; 0], time);

        let quote_bytes = include_bytes!("../data/tests/hw_quote.dat");
        let quote = Quote3::try_from(quote_bytes.to_vec()).expect("Failed to parse quote");
        let collateral = collateral(TCB_INFO_JSON, QE_IDENTITY_JSON);
        let evidence = Evidence::new(quote, collateral).expect("Failed to create evidence");

        let verification = verifier.verify(&evidence);

        assert_eq!(verification.is_success().unwrap_u8(), 0);

        let displayable = VerificationTreeDisplay::new(&verifier, verification);
        let expected = r#"
            - [ ] all of the following must be true:
              - [x] The TCB issuer chain was verified.
              - [x] The QE identity issuer chain was verified.
              - [x] The Quote issuer chain was verified.
              - [ ] The TCB info could not be verified: TCB info expired"#;
        assert_eq!(format!("\n{displayable}"), textwrap::dedent(expected));
    }
}

// Copyright (c) 2018-2023 The MobileCoin Foundation

//! Verifiers which operate on the [`ReportBody`]

use crate::{VerificationError, Verifier};
use core::fmt::Debug;
use mc_sgx_core_types::{Attributes, ConfigId, ReportBody};
use subtle::CtOption;

/// Trait for getting access to the type `T` that needs to be verified.
///
/// The intent is to implement this for a higher level type that contains the
/// `T`
///
/// ```ignore
/// use mc_attestation_verifier::ConfigIdVerifier;
/// impl Accessor<Contained> for Container {
///     fn get(&self) -> Contained {
///         self.some_method()
///     }
/// }
/// ```
pub trait Accessor<T>: Debug {
    fn get(&self) -> T;
}

impl Accessor<Attributes> for ReportBody {
    fn get(&self) -> Attributes {
        self.attributes()
    }
}

impl Accessor<Attributes> for Attributes {
    fn get(&self) -> Attributes {
        *self
    }
}

/// Verify the attributes are as expected.
#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub struct AttributesVerifier {
    expected_attributes: Attributes,
}

impl AttributesVerifier {
    /// Create a new [`AttributesVerifier`] instance.
    ///
    /// # Arguments:
    /// * `expected_attributes` - The expected attributes.
    /// * `report_body` - The report body containing attributes that conforms
    ///    to the `expected_attributes`.
    pub fn new(expected_attributes: Attributes) -> Self {
        Self {
            expected_attributes,
        }
    }
}

impl<T: Accessor<Attributes>> Verifier<T> for AttributesVerifier {
    type Error = VerificationError;
    fn verify(&self, evidence: &T) -> CtOption<Self::Error> {
        let expected = self.expected_attributes;
        let actual = evidence.get();
        // TODO - This should be a constant time comparison.
        let is_some = if expected == actual { 0 } else { 1 };
        CtOption::new(
            VerificationError::AttributeMismatch { expected, actual },
            is_some.into(),
        )
    }
}

impl Accessor<ConfigId> for ReportBody {
    fn get(&self) -> ConfigId {
        self.config_id()
    }
}

impl Accessor<ConfigId> for ConfigId {
    fn get(&self) -> ConfigId {
        self.clone()
    }
}

/// Verify the [`ConfigId`] is as expected.
#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub struct ConfigIdVerifier {
    expected_id: ConfigId,
}

impl ConfigIdVerifier {
    /// Create a new [`ConfigIdVerifier`] instance.
    ///
    /// # Arguments:
    /// * `expected_id` - The expected id.
    /// * `report_body` - The report body containing config id that conforms
    ///    to the `expected_id`.
    pub fn new(expected_id: ConfigId) -> Self {
        Self { expected_id }
    }
}

impl<T: Accessor<ConfigId>> Verifier<T> for ConfigIdVerifier {
    type Error = VerificationError;
    fn verify(&self, evidence: &T) -> CtOption<Self::Error> {
        let expected = self.expected_id.clone();
        let actual = evidence.get();
        // TODO - This should be a constant time comparison.
        let is_some = if expected == actual { 0 } else { 1 };
        CtOption::new(
            VerificationError::ConfigIdMismatch { expected, actual },
            is_some.into(),
        )
    }
}

#[cfg(test)]
mod test {
    use super::*;
    use crate::And;
    use mc_sgx_core_sys_types::{
        sgx_attributes_t, sgx_cpu_svn_t, sgx_measurement_t, sgx_report_body_t, sgx_report_data_t,
    };

    const REPORT_BODY_SRC: sgx_report_body_t = sgx_report_body_t {
        cpu_svn: sgx_cpu_svn_t {
            svn: [1u8, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15, 16],
        },
        misc_select: 17,
        reserved1: [0u8; 12],
        isv_ext_prod_id: [1u8, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15, 16],
        attributes: sgx_attributes_t {
            flags: 0x0102_0304_0506_0708,
            xfrm: 0x0807_0605_0403_0201,
        },
        mr_enclave: sgx_measurement_t {
            m: [
                17, 18, 19, 20, 21, 22, 23, 24, 25, 26, 27, 28, 29, 30, 31, 32, 33, 34, 35, 36, 37,
                38, 39, 40, 41, 42, 43, 43, 44, 45, 46, 47,
            ],
        },
        reserved2: [0u8; 32],
        mr_signer: sgx_measurement_t {
            m: [
                48, 49, 50, 51, 52, 53, 54, 55, 56, 57, 58, 59, 60, 61, 62, 63, 64, 65, 66, 67, 68,
                69, 70, 71, 72, 73, 74, 75, 76, 77, 78, 79,
            ],
        },
        reserved3: [0u8; 32],
        config_id: [
            80, 81, 82, 83, 84, 85, 86, 87, 88, 89, 90, 91, 92, 93, 94, 95, 96, 97, 98, 99, 100,
            101, 102, 103, 104, 105, 106, 107, 108, 109, 110, 111, 112, 113, 114, 115, 116, 117,
            118, 119, 120, 121, 122, 123, 124, 125, 126, 127, 128, 129, 130, 131, 132, 133, 134,
            135, 136, 137, 138, 139, 140, 141, 142, 143,
        ],
        isv_prod_id: 144,
        isv_svn: 145,
        config_svn: 146,
        reserved4: [0u8; 42],
        isv_family_id: [
            147, 148, 149, 150, 151, 152, 153, 154, 155, 156, 157, 158, 159, 160, 161, 162,
        ],
        report_data: sgx_report_data_t {
            d: [
                163, 164, 165, 166, 167, 168, 169, 170, 171, 172, 173, 174, 175, 176, 177, 178,
                179, 180, 181, 182, 183, 184, 185, 186, 187, 188, 189, 190, 191, 192, 193, 194,
                195, 196, 197, 198, 199, 200, 201, 202, 203, 204, 205, 206, 207, 208, 209, 210,
                211, 212, 213, 214, 215, 216, 217, 218, 219, 220, 221, 222, 223, 224, 225, 226,
            ],
        },
    };

    #[test]
    fn report_body_succeeds() {
        let report_body = ReportBody::from(&REPORT_BODY_SRC);
        let verifier = And::new(
            AttributesVerifier::new(report_body.attributes()),
            ConfigIdVerifier::new(report_body.config_id()),
        );
        assert_eq!(verifier.verify(&report_body).is_none().unwrap_u8(), 1);
    }

    #[test]
    fn report_body_fails_due_to_attributes() {
        let report_body = ReportBody::from(&REPORT_BODY_SRC);
        let attributes = report_body.attributes().set_flags(0);
        let verifier = And::new(
            AttributesVerifier::new(attributes),
            ConfigIdVerifier::new(report_body.config_id()),
        );
        assert_eq!(verifier.verify(&report_body).is_some().unwrap_u8(), 1);
    }

    #[test]
    fn report_body_fails_due_to_config_id() {
        let report_body = ReportBody::from(&REPORT_BODY_SRC);
        let mut config_id = report_body.config_id();
        config_id.as_mut()[0] = 0;
        let verifier = And::new(
            AttributesVerifier::new(report_body.attributes()),
            ConfigIdVerifier::new(config_id),
        );
        assert_eq!(verifier.verify(&report_body).is_some().unwrap_u8(), 1);
    }

    #[test]
    fn attributes_success() {
        let attributes = Attributes::from(REPORT_BODY_SRC.attributes);
        let verifier = AttributesVerifier::new(attributes);

        assert_eq!(verifier.verify(&attributes).is_none().unwrap_u8(), 1);
    }

    #[test]
    fn attributes_fail() {
        let mut attributes = Attributes::from(REPORT_BODY_SRC.attributes);
        let verifier = AttributesVerifier::new(attributes);
        attributes = attributes.set_flags(0);

        assert_eq!(verifier.verify(&attributes).is_some().unwrap_u8(), 1);
    }

    #[test]
    fn config_id_success() {
        let config_id = ConfigId::from(REPORT_BODY_SRC.config_id);
        let verifier = ConfigIdVerifier::new(config_id.clone());

        assert_eq!(verifier.verify(&config_id).is_none().unwrap_u8(), 1);
    }

    #[test]
    fn config_id_fail() {
        let mut config_id = ConfigId::from(REPORT_BODY_SRC.config_id);
        let verifier = ConfigIdVerifier::new(config_id.clone());
        config_id.as_mut()[0] = 2;

        assert_eq!(verifier.verify(&config_id).is_some().unwrap_u8(), 1);
    }
}

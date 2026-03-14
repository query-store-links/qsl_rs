use serde::{Deserialize, Serialize};

// ─── Request ─────────────────────────────────────────────────────────────────

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ResolveAllRequest {
    #[serde(alias = "ProductInput")]
    pub product_input: Option<String>,
    #[serde(default = "default_locale", alias = "Locale")]
    pub locale: String,
    #[serde(default = "default_market", alias = "Market")]
    pub market: String,
    #[serde(default = "default_id_type", alias = "IdentifierType")]
    pub identifier_type: String,
}

fn default_locale() -> String {
    "en-US".into()
}
fn default_market() -> String {
    "US".into()
}
fn default_id_type() -> String {
    "ProductId".into()
}

// ─── Response ────────────────────────────────────────────────────────────────

#[derive(Debug, Serialize, Default)]
#[serde(rename_all = "PascalCase")]
pub struct ResolveAllResponse {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub product_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub app_info: Option<AppInfo>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub appx_packages: Option<Vec<DownloadItem>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub non_appx_packages: Option<Vec<DownloadItem>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub errors: Option<Vec<String>>,
}

impl ResolveAllResponse {
    pub fn error(msg: impl Into<String>) -> Self {
        Self {
            errors: Some(vec![msg.into()]),
            ..Default::default()
        }
    }
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "PascalCase")]
pub struct AppInfo {
    pub name: String,
    pub publisher: String,
    pub description: String,
    pub category_id: Option<String>,
    pub product_id: Option<String>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "PascalCase")]
pub struct DownloadItem {
    pub file_name: String,
    pub file_link: String,
    pub file_size: String,
}

// ─── StoreEdgeFD types (Non-AppX path) ───────────────────────────────────────

#[derive(Debug, Deserialize)]
#[serde(rename_all = "PascalCase")]
pub struct PackageManifestResponse {
    pub data: Option<PackageManifestData>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "PascalCase")]
pub struct PackageManifestData {
    pub package_identifier: String,
    pub versions: Vec<PackageManifestVersion>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "PascalCase")]
pub struct PackageManifestVersion {
    pub default_locale: Option<DefaultLocale>,
    pub installers: Vec<SparkInstaller>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "PascalCase")]
pub struct DefaultLocale {
    pub package_name: Option<String>,
    pub publisher: Option<String>,
    pub short_description: Option<String>,
    pub agreements: Option<Vec<AgreementDetail>>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "PascalCase")]
pub struct AgreementDetail {
    pub agreement_label: Option<String>,
    pub agreement: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "PascalCase")]
pub struct SparkInstaller {
    pub installer_url: String,
    pub architecture: String,
    pub installer_type: String,
}

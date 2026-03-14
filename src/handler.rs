use crate::{
    models::{AppInfo, DownloadItem, ResolveAllRequest, ResolveAllResponse},
    package_helper,
};
use axum::{
    extract::{Json, State},
    http::StatusCode,
    response::{IntoResponse, Response},
};
use futures::future::join_all;
use std::sync::Arc;
use storelib_rs::{DCatEndpoint, DisplayCatalogHandler, IdentifierType, Lang, Locale, Market};
use tracing::{info, warn};

pub struct AppState {
    pub client: reqwest::Client,
}

pub async fn resolve_all(
    State(state): State<Arc<AppState>>,
    body: Option<Json<ResolveAllRequest>>,
) -> Response {
    let req = match body {
        None => {
            return (StatusCode::BAD_REQUEST, "Request body is required.").into_response();
        }
        Some(Json(r)) => r,
    };

    let product_input = match req.product_input.as_deref() {
        None | Some("") => {
            return (StatusCode::BAD_REQUEST, "ProductInput is required.").into_response();
        }
        Some(s) => s.to_string(),
    };

    if product_input.to_ascii_lowercase().starts_with("xp") {
        info!("Detected Non-Appx ID: {product_input}");
        return handle_non_appx(&state.client, &product_input, &req.locale, &req.market).await;
    }

    handle_appx(&state.client, &product_input, &req).await
}

// ─── Non-AppX path ───────────────────────────────────────────────────────────

async fn handle_non_appx(
    client: &reqwest::Client,
    product_id: &str,
    locale: &str,
    market: &str,
) -> Response {
    let url = format!(
        "http://storeedgefd.dsx.mp.microsoft.com/v9.0/packageManifests/{}?locale={}&market={}",
        product_id.to_ascii_lowercase(),
        locale.to_ascii_lowercase(),
        market.to_ascii_uppercase(),
    );

    let manifest = match client.get(&url).send().await {
        Ok(r) if r.status().is_success() => {
            match r.json::<crate::models::PackageManifestResponse>().await {
                Ok(m) => m,
                Err(_) => {
                    return Json(ResolveAllResponse::error("Non-Appx product not found."))
                        .into_response()
                }
            }
        }
        _ => return Json(ResolveAllResponse::error("Non-Appx product not found.")).into_response(),
    };

    let data = match manifest.data {
        Some(d) => d,
        None => {
            return Json(ResolveAllResponse::error("Non-Appx product not found.")).into_response()
        }
    };

    let version = match data.versions.into_iter().next() {
        Some(v) => v,
        None => {
            return Json(ResolveAllResponse::error("Non-Appx product not found.")).into_response()
        }
    };

    let loc = version.default_locale.as_ref();
    let app_name = loc
        .and_then(|l| l.package_name.as_deref())
        .unwrap_or("Unknown")
        .to_string();

    let app_info = AppInfo {
        name: app_name.clone(),
        publisher: loc
            .and_then(|l| l.publisher.as_deref())
            .unwrap_or("Unknown")
            .to_string(),
        description: loc
            .and_then(|l| l.short_description.as_deref())
            .unwrap_or("")
            .to_string(),
        category_id: loc.and_then(|l| {
            l.agreements
                .as_ref()?
                .iter()
                .find(|a| a.agreement_label.as_deref() == Some("Category"))?
                .agreement
                .clone()
        }),
        product_id: Some(data.package_identifier.clone()),
    };

    let download_tasks = version.installers.into_iter().map(|installer| {
        let client = client.clone();
        let name = format!(
            "{}_{}. {}",
            app_name, installer.architecture, installer.installer_type
        );
        let url = installer.installer_url;
        async move {
            let size = package_helper::get_file_size(&client, &url).await;
            DownloadItem {
                file_name: name,
                file_link: url,
                file_size: size,
            }
        }
    });

    let downloads = join_all(download_tasks).await;

    Json(ResolveAllResponse {
        product_id: Some(product_id.to_ascii_uppercase()),
        app_info: Some(app_info),
        non_appx_packages: Some(downloads),
        ..Default::default()
    })
    .into_response()
}

// ─── AppX (Display Catalog) path ─────────────────────────────────────────────

async fn handle_appx(
    client: &reqwest::Client,
    product_input: &str,
    req: &ResolveAllRequest,
) -> Response {
    let locale = parse_locale(&req.locale, &req.market);
    let id_type = parse_identifier_type(&req.identifier_type);

    info!(
        "Querying DCAT: Input={product_input}, Type={id_type:?}, Market={}",
        req.market
    );

    let mut handler = DisplayCatalogHandler::new(DCatEndpoint::Production, locale);

    if let Err(_) = handler
        .query_dcat(product_input, id_type.clone(), None)
        .await
    {
        warn!(
            "No product found for Input={product_input}, Type={id_type:?}, Market={}",
            req.market
        );
        return Json(ResolveAllResponse::error("Product not found.")).into_response();
    }

    // Extract all product data we need before calling get_packages_for_product,
    // so that the immutable borrow of handler.product_listing ends before the
    // mutable/exclusive access needed by the next async call.
    let (product_id, app_info) = {
        let listing = match handler.product_listing.as_ref() {
            Some(l) => l,
            None => return Json(ResolveAllResponse::error("Product not found.")).into_response(),
        };

        let product = match listing
            .products
            .as_deref()
            .and_then(|v| v.first())
            .or(listing.product.as_ref())
        {
            Some(p) => p,
            None => {
                warn!("No product found for Input={product_input}");
                return Json(ResolveAllResponse::error("Product not found.")).into_response();
            }
        };

        let locale_props = product
            .localized_properties
            .as_deref()
            .and_then(|v| v.first());

        let sku_props = product
            .display_sku_availabilities
            .as_deref()
            .and_then(|v| v.first())
            .and_then(|dsa| dsa.sku.as_ref())
            .and_then(|sku| sku.properties.as_ref());

        if sku_props.is_none() {
            return Json(ResolveAllResponse::error("SKU properties not found.")).into_response();
        }
        let sku_props = sku_props.unwrap();

        let product_id = sku_props
            .fulfillment_data
            .as_ref()
            .and_then(|fd| fd.product_id.clone())
            .unwrap_or_else(|| product_input.to_string());

        let app_info = AppInfo {
            name: locale_props
                .and_then(|l| l.product_title.clone())
                .unwrap_or_else(|| "Unknown Name".into()),
            publisher: locale_props
                .and_then(|l| l.publisher_name.clone())
                .unwrap_or_else(|| "Unknown Publisher".into()),
            description: locale_props
                .and_then(|l| l.product_description.clone())
                .unwrap_or_default(),
            category_id: sku_props
                .fulfillment_data
                .as_ref()
                .and_then(|fd| fd.wu_category_id.clone()),
            product_id: Some(product_id.clone()),
        };

        (product_id, app_info)
    }; // borrow of handler.product_listing ends here

    let packages = match handler.get_packages_for_product(None).await {
        Ok(p) => p,
        Err(e) => {
            warn!("Failed to get packages for {product_input}: {e:?}");
            return Json(ResolveAllResponse {
                product_id: Some(product_id),
                app_info: Some(app_info),
                errors: Some(vec![format!("Failed to fetch packages: {e:?}")]),
                ..Default::default()
            })
            .into_response();
        }
    };

    let download_tasks = packages.into_iter().map(|pkg| {
        let client = client.clone();
        let uri = pkg.package_uri.unwrap_or_default();
        let moniker = pkg.package_moniker;
        async move {
            let (size, name) = if uri.is_empty() {
                ("Unknown".to_string(), moniker)
            } else {
                let size = package_helper::get_file_size(&client, &uri).await;
                let name = package_helper::get_file_name(&client, &uri).await;
                let name = if name.is_empty() { moniker } else { name };
                (size, name)
            };
            DownloadItem {
                file_name: name,
                file_link: uri,
                file_size: size,
            }
        }
    });

    let appx_packages = join_all(download_tasks).await;

    Json(ResolveAllResponse {
        product_id: Some(product_id),
        app_info: Some(app_info),
        appx_packages: Some(appx_packages),
        ..Default::default()
    })
    .into_response()
}

// ─── Locale / identifier parsing ─────────────────────────────────────────────

fn parse_locale(locale_str: &str, market_str: &str) -> Locale {
    let lang_code = locale_str
        .split('-')
        .next()
        .unwrap_or("en")
        .to_ascii_lowercase();
    let market_code = market_str.to_ascii_uppercase();

    let lang = match lang_code.as_str() {
        "ar" => Lang::Ar,
        "az" => Lang::Az,
        "be" => Lang::Be,
        "bg" => Lang::Bg,
        "bn" => Lang::Bn,
        "bs" => Lang::Bs,
        "ca" => Lang::Ca,
        "cs" => Lang::Cs,
        "da" => Lang::Da,
        "de" => Lang::De,
        "el" => Lang::El,
        "es" => {
            if locale_str.eq_ignore_ascii_case("es-MX") {
                Lang::EsMx
            } else {
                Lang::Es
            }
        }
        "et" => Lang::Et,
        "eu" => Lang::Eu,
        "fa" => Lang::Fa,
        "fi" => Lang::Fi,
        "fr" => {
            if locale_str.eq_ignore_ascii_case("fr-CA") {
                Lang::FrCa
            } else {
                Lang::Fr
            }
        }
        "gl" => Lang::Gl,
        "gu" => Lang::Gu,
        "he" => Lang::He,
        "hi" => Lang::Hi,
        "hr" => Lang::Hr,
        "hu" => Lang::Hu,
        "hy" => Lang::Hy,
        "id" => Lang::Id,
        "is" => Lang::Is,
        "it" => Lang::It,
        "ja" => Lang::Ja,
        "ka" => Lang::Ka,
        "kk" => Lang::Kk,
        "km" => Lang::Km,
        "kn" => Lang::Kn,
        "ko" => Lang::Ko,
        "lt" => Lang::Lt,
        "lv" => Lang::Lv,
        "mk" => Lang::Mk,
        "ml" => Lang::Ml,
        "mr" => Lang::Mr,
        "ms" => Lang::Ms,
        "nb" => Lang::Nb,
        "nl" => Lang::Nl,
        "or" => Lang::Or,
        "pa" => Lang::Pa,
        "pl" => Lang::Pl,
        "pt" => {
            if locale_str.eq_ignore_ascii_case("pt-BR") {
                Lang::PtBr
            } else {
                Lang::Pt
            }
        }
        "ro" => Lang::Ro,
        "ru" => Lang::Ru,
        "sk" => Lang::Sk,
        "sl" => Lang::Sl,
        "sr" => {
            if locale_str.to_ascii_lowercase().contains("latn") {
                Lang::SrLatn
            } else {
                Lang::Sr
            }
        }
        "sv" => Lang::Sv,
        "te" => Lang::Te,
        "tg" => Lang::Tg,
        "th" => Lang::Th,
        "tr" => Lang::Tr,
        "uk" => Lang::Uk,
        "ur" => Lang::Ur,
        "uz" => Lang::Uz,
        "vi" => Lang::Vi,
        "zh" => {
            if locale_str.to_ascii_lowercase().contains("hant")
                || matches!(market_code.as_str(), "TW" | "HK")
            {
                Lang::ZhHant
            } else {
                Lang::ZhHans
            }
        }
        "en" => {
            if locale_str.eq_ignore_ascii_case("en-GB") {
                Lang::EnGb
            } else {
                Lang::En
            }
        }
        _ => Lang::En,
    };

    let market = match market_code.as_str() {
        "AF" => Market::Af,
        "AL" => Market::Al,
        "DZ" => Market::Dz,
        "AO" => Market::Ao,
        "AR" => Market::Ar,
        "AM" => Market::Am,
        "AU" => Market::Au,
        "AT" => Market::At,
        "AZ" => Market::Az,
        "BS" => Market::Bs,
        "BH" => Market::Bh,
        "BD" => Market::Bd,
        "BE" => Market::Be,
        "BZ" => Market::Bz,
        "BO" => Market::Bo,
        "BA" => Market::Ba,
        "BW" => Market::Bw,
        "BR" => Market::Br,
        "BN" => Market::Bn,
        "BG" => Market::Bg,
        "CM" => Market::Cm,
        "CA" => Market::Ca,
        "CV" => Market::Cv,
        "CL" => Market::Cl,
        "CO" => Market::Co,
        "CR" => Market::Cr,
        "HR" => Market::Hr,
        "CY" => Market::Cy,
        "CZ" => Market::Cz,
        "DK" => Market::Dk,
        "DO" => Market::Do,
        "EC" => Market::Ec,
        "EG" => Market::Eg,
        "SV" => Market::Sv,
        "ET" => Market::Et,
        "EE" => Market::Ee,
        "FJ" => Market::Fj,
        "FI" => Market::Fi,
        "FR" => Market::Fr,
        "GE" => Market::Ge,
        "DE" => Market::De,
        "GH" => Market::Gh,
        "GR" => Market::Gr,
        "GT" => Market::Gt,
        "HK" => Market::Hk,
        "HN" => Market::Hn,
        "HU" => Market::Hu,
        "IS" => Market::Is,
        "IN" => Market::In,
        "ID" => Market::Id,
        "IQ" => Market::Iq,
        "IE" => Market::Ie,
        "IL" => Market::Il,
        "IT" => Market::It,
        "JM" => Market::Jm,
        "JP" => Market::Jp,
        "JO" => Market::Jo,
        "KZ" => Market::Kz,
        "KE" => Market::Ke,
        "KW" => Market::Kw,
        "KG" => Market::Kg,
        "LV" => Market::Lv,
        "LB" => Market::Lb,
        "LI" => Market::Li,
        "LT" => Market::Lt,
        "LU" => Market::Lu,
        "MY" => Market::My,
        "MV" => Market::Mv,
        "MT" => Market::Mt,
        "MX" => Market::Mx,
        "MN" => Market::Mn,
        "MA" => Market::Ma,
        "MZ" => Market::Mz,
        "NG" => Market::Ng,
        "NI" => Market::Ni,
        "NP" => Market::Np,
        "NL" => Market::Nl,
        "NZ" => Market::Nz,
        "NO" => Market::No,
        "OM" => Market::Om,
        "PK" => Market::Pk,
        "PA" => Market::Pa,
        "PY" => Market::Py,
        "PE" => Market::Pe,
        "PH" => Market::Ph,
        "PL" => Market::Pl,
        "PT" => Market::Pt,
        "QA" => Market::Qa,
        "RO" => Market::Ro,
        "RU" => Market::Ru,
        "SA" => Market::Sa,
        "SN" => Market::Sn,
        "SG" => Market::Sg,
        "SK" => Market::Sk,
        "SI" => Market::Si,
        "ZA" => Market::Za,
        "KR" => Market::Kr,
        "ES" => Market::Es,
        "LK" => Market::Lk,
        "SE" => Market::Se,
        "CH" => Market::Ch,
        "TW" => Market::Tw,
        "TJ" => Market::Tj,
        "TZ" => Market::Tz,
        "TH" => Market::Th,
        "TT" => Market::Tt,
        "TN" => Market::Tn,
        "TR" => Market::Tr,
        "TM" => Market::Tm,
        "UG" => Market::Ug,
        "UA" => Market::Ua,
        "AE" => Market::Ae,
        "GB" => Market::Gb,
        "US" => Market::Us,
        "UY" => Market::Uy,
        "UZ" => Market::Uz,
        "VE" => Market::Ve,
        _ => Market::Us,
    };

    Locale::new(market, lang, true)
}

fn parse_identifier_type(s: &str) -> IdentifierType {
    match s {
        "PackageFamilyName" => IdentifierType::PackageFamilyName,
        "ContentId" => IdentifierType::ContentId,
        "XboxTitleId" => IdentifierType::XboxTitleId,
        "LegacyWindowsPhoneProductId" => IdentifierType::LegacyWindowsPhoneProductId,
        "LegacyWindowsStoreProductId" => IdentifierType::LegacyWindowsStoreProductId,
        "LegacyXboxProductId" => IdentifierType::LegacyXboxProductId,
        _ => IdentifierType::ProductId,
    }
}

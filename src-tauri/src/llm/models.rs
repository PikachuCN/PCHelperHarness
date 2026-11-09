use reqwest::Client;
use serde::Deserialize;

use super::types::ModelInfo;
use super::{api_request, checked_response, endpoint};
use crate::config::ProviderConfig;

pub(crate) async fn fetch(
    client: &Client,
    provider: &ProviderConfig,
    api_key: &str,
) -> Result<Vec<ModelInfo>, super::LlmError> {
    let request = api_request(
        client.get(endpoint(&provider.base_url, "models")),
        provider,
        api_key,
    );
    let response: ModelsResponse = checked_response(request, api_key).await?.json().await?;
    Ok(collect_models(response, provider.is_kilo_free()))
}

fn collect_models(response: ModelsResponse, free_only: bool) -> Vec<ModelInfo> {
    response
        .data
        .into_iter()
        .filter(|model| !free_only || model.is_free == Some(true))
        .map(|model| ModelInfo {
            id: model.id,
            name: model.display_name.or(model.name),
            owned_by: model.owned_by,
        })
        .collect()
}

#[derive(Debug, Deserialize)]
struct ModelsResponse {
    #[serde(default)]
    data: Vec<RemoteModel>,
}

#[derive(Debug, Deserialize)]
struct RemoteModel {
    id: String,
    #[serde(default, rename = "isFree")]
    is_free: Option<bool>,
    #[serde(default)]
    name: Option<String>,
    #[serde(default)]
    display_name: Option<String>,
    #[serde(default)]
    owned_by: Option<String>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn accepts_openai_model_list_shape() {
        let response: ModelsResponse = serde_json::from_str(
            r#"{"data":[{"id":"gpt-test","owned_by":"acme","display_name":"Test"}]}"#,
        )
        .unwrap();
        assert_eq!(response.data[0].id, "gpt-test");
        assert_eq!(response.data[0].display_name.as_deref(), Some("Test"));
    }

    #[test]
    fn kilo_listing_only_exposes_explicitly_free_models() {
        let response: ModelsResponse = serde_json::from_str(
            r#"{"data":[{"id":"kilo-auto/free","name":"Auto Free","isFree":true},{"id":"paid/model","isFree":false},{"id":"unknown/model"}]}"#,
        )
        .unwrap();
        let models = collect_models(response, true);
        assert_eq!(models.len(), 1);
        assert_eq!(models[0].id, "kilo-auto/free");
        assert_eq!(models[0].name.as_deref(), Some("Auto Free"));
    }
}

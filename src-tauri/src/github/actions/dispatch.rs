use super::{
    client::{checked, Client},
    model,
    transport::{Request, Transport},
};
use crate::github::enc;
use crate::github::http::{Error, Response};
use crate::kernel::ci::*;
use serde_json::{json, Value};
use std::collections::BTreeMap;

impl<T: Transport> Client<'_, T> {
    pub(super) async fn dispatch_form(
        &self,
        request: &CiDispatch,
    ) -> Result<CiDispatchForm, CiError> {
        validate_request(request)?;
        let yaml = self.configuration(request).await?;
        let inputs = parse_inputs(&yaml)?;
        Ok(CiDispatchForm {
            provider: model::PROVIDER.into(),
            host: self.repo.host.clone(),
            pipeline_id: request.pipeline_id.clone(),
            reference: request.reference.clone(),
            capabilities: CiCapabilities {
                can_dispatch: true,
                can_rerun_failed: true,
            },
            inputs,
        })
    }

    async fn configuration_path(&self, pipeline: &str) -> Result<String, CiError> {
        let path = format!(
            "{}/actions/workflows/{}",
            self.repo.api_path(),
            enc(pipeline)
        );
        let response = checked(
            self.transport
                .send(Request::Metadata {
                    path: &path,
                    etag: None,
                })
                .await
                .map_err(CiError::from)?,
            false,
        )?;
        let value: Value = serde_json::from_slice(&response.body)
            .map_err(|_| CiError::message("Unexpected CI pipeline from GitHub"))?;
        let path = value
            .get("path")
            .and_then(Value::as_str)
            .ok_or_else(|| CiError::message("CI pipeline has no configuration path"))?;
        if !path.starts_with(".github/workflows/")
            || path
                .split('/')
                .any(|segment| segment == ".." || segment.is_empty())
        {
            return Err(CiError::message("Invalid CI pipeline configuration path"));
        }
        Ok(path.to_string())
    }

    async fn configuration(&self, request: &CiDispatch) -> Result<Value, CiError> {
        let configuration_path = self.configuration_path(&request.pipeline_id).await?;
        let path = format!(
            "{}/contents/{}?ref={}",
            self.repo.api_path(),
            configuration_path
                .split('/')
                .map(enc)
                .collect::<Vec<_>>()
                .join("/"),
            enc(&request.reference)
        );
        let response = checked_contents(
            self.transport
                .send(Request::Raw { path: &path })
                .await
                .map_err(CiError::from)?,
        )?;
        if response.body.len() > 1024 * 1024 {
            return Err(CiError::message("CI pipeline configuration exceeds 1 MiB"));
        }
        serde_yaml_ng::from_slice(&response.body)
            .map_err(|_| CiError::message("Cannot parse CI pipeline configuration"))
    }

    pub(super) async fn dispatch(&self, request: &CiDispatch) -> Result<CiActionResult, CiError> {
        let form = self.dispatch_form(request).await?;
        let inputs = validate_inputs(&form.inputs, &request.inputs)?;
        let path = format!(
            "{}/actions/workflows/{}/dispatches",
            self.repo.api_path(),
            enc(&request.pipeline_id)
        );
        self.write(
            &path,
            Some(json!({"ref":request.reference,"inputs":inputs})),
        )
        .await?;
        Ok(self.receipt(None))
    }
}

fn checked_contents(response: Response) -> Result<Response, CiError> {
    match response.checked() {
        Err(Error::Http { status: 401 | 403 | 404, .. }) => Err(CiError::message("CI dispatch inputs need repository Contents read permission; check the chosen ref and stored GitHub token")),
        result => result.map_err(CiError::from),
    }
}

fn validate_request(request: &CiDispatch) -> Result<(), CiError> {
    crate::git::valid_ref(&request.reference).map_err(CiError::message)?;
    if request.reference.len() > 1024
        || request.pipeline_id.is_empty()
        || request.pipeline_id.len() > 255
        || request
            .pipeline_id
            .chars()
            .any(|value| value.is_control() || value == '/' || value == '\\')
    {
        return Err(CiError::message("Invalid CI pipeline or ref"));
    }
    if request.inputs.len() > 25
        || serde_json::to_vec(&request.inputs)
            .map_err(|_| CiError::message("Invalid CI inputs"))?
            .len()
            > 65_535
    {
        return Err(CiError::message(
            "CI dispatch inputs exceed the supported limit",
        ));
    }
    Ok(())
}

fn parse_inputs(yaml: &Value) -> Result<Vec<CiInput>, CiError> {
    let event = yaml
        .get("on")
        .ok_or_else(|| CiError::message("CI pipeline does not support manual dispatch"))?;
    let dispatch = event.get("workflow_dispatch");
    let declared = event.as_str() == Some("workflow_dispatch")
        || event.as_array().is_some_and(|events| {
            events
                .iter()
                .any(|value| value.as_str() == Some("workflow_dispatch"))
        });
    if dispatch.is_none() && !declared {
        return Err(CiError::message(
            "CI pipeline does not support manual dispatch",
        ));
    }
    let Some(inputs) = dispatch.and_then(|value| value.get("inputs")) else {
        return Ok(Vec::new());
    };
    let inputs = inputs
        .as_object()
        .ok_or_else(|| CiError::message("Invalid CI dispatch input declarations"))?;
    inputs
        .iter()
        .map(|(name, input)| parse_input(name, input))
        .collect()
}

fn parse_input(name: &str, input: &Value) -> Result<CiInput, CiError> {
    let kind = match input
        .get("type")
        .and_then(Value::as_str)
        .unwrap_or("string")
    {
        "string" | "environment" => CiInputKind::String,
        "boolean" => CiInputKind::Boolean,
        "number" => CiInputKind::Number,
        "choice" => CiInputKind::Choice,
        _ => return Err(CiError::message("Unsupported CI dispatch input type")),
    };
    let options = input
        .get("options")
        .map(|options| {
            serde_json::from_value(options.clone())
                .map_err(|_| CiError::message("Invalid CI input choices"))
        })
        .transpose()?
        .unwrap_or_default();
    Ok(CiInput {
        name: name.into(),
        description: input
            .get("description")
            .and_then(Value::as_str)
            .unwrap_or("")
            .into(),
        kind,
        required: input
            .get("required")
            .and_then(Value::as_bool)
            .unwrap_or(false),
        default: input.get("default").cloned(),
        options,
    })
}

fn validate_inputs(
    schema: &[CiInput],
    supplied: &BTreeMap<String, Value>,
) -> Result<BTreeMap<String, Value>, CiError> {
    if supplied
        .keys()
        .any(|name| !schema.iter().any(|input| &input.name == name))
    {
        return Err(CiError::message("Unknown CI dispatch input"));
    }
    let mut inputs = BTreeMap::new();
    for input in schema {
        let value = supplied.get(&input.name).or(input.default.as_ref());
        let Some(value) = value else {
            if input.required {
                return Err(CiError::message(format!(
                    "Missing required CI input {}",
                    input.name
                )));
            }
            continue;
        };
        let valid = match input.kind {
            CiInputKind::String => value.is_string(),
            CiInputKind::Boolean => value.is_boolean(),
            CiInputKind::Number => value.is_number(),
            CiInputKind::Choice => value
                .as_str()
                .is_some_and(|value| input.options.iter().any(|option| option == value)),
        };
        if !valid || input.required && value.as_str() == Some("") {
            return Err(CiError::message(format!("Invalid CI input {}", input.name)));
        }
        inputs.insert(input.name.clone(), value.clone());
    }
    Ok(inputs)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_typed_yaml_dispatch_inputs_and_rejects_invalid_values() {
        let yaml: Value = serde_yaml_ng::from_str("on:\n  workflow_dispatch:\n    inputs:\n      target:\n        type: choice\n        required: true\n        options: [staging, production]\n      dry:\n        type: boolean\n        default: true\n      count:\n        type: number\n").unwrap();
        let schema = parse_inputs(&yaml).unwrap();
        let inputs = BTreeMap::from([
            ("target".into(), json!("staging")),
            ("count".into(), json!(3)),
        ]);
        let values = validate_inputs(&schema, &inputs).unwrap();
        assert_eq!(values["dry"], true);
        for inputs in [
            BTreeMap::new(),
            BTreeMap::from([("target".into(), json!("other"))]),
            BTreeMap::from([
                ("target".into(), json!("staging")),
                ("dry".into(), json!("true")),
            ]),
            BTreeMap::from([("unexpected".into(), json!("value"))]),
        ] {
            assert!(validate_inputs(&schema, &inputs).is_err());
        }
    }
}

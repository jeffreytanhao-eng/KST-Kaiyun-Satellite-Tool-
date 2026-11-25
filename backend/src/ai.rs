use std::time::Duration;

use serde::{Deserialize, Serialize};

use crate::config::Config;

/// 兼容 OpenAI Chat Completions 协议的 AI 适配器。
#[derive(Clone)]
pub struct AiClient {
    enabled: bool,
    base_url: String,
    api_key: String,
    model: String,
    http: reqwest::Client,
}

#[derive(Debug, Clone, Serialize)]
pub struct AiPreset {
    pub id: &'static str,
    pub name: &'static str,
    pub name_en: &'static str,
    pub description: &'static str,
    pub description_en: &'static str,
}

pub const PRESETS: [AiPreset; 4] = [
    AiPreset {
        id: "access_interpret",
        name: "解读 Access 结果",
        name_en: "Interpret access results",
        description: "分析可见窗口分布、时长与约束影响，给出任务可用性结论。",
        description_en: "Analyze access window distribution, durations and constraint impact.",
    },
    AiPreset {
        id: "coverage_interpret",
        name: "评估区域覆盖",
        name_en: "Assess regional coverage",
        description: "解读覆盖率、盲区与重访能力，给出改进建议。",
        description_en: "Interpret coverage ratio, gaps and revisit capability.",
    },
    AiPreset {
        id: "environment_interpret",
        name: "解读大气环境影响",
        name_en: "Interpret atmospheric impact",
        description: "分析密度、阻力与高度衰减对轨道寿命的影响。",
        description_en: "Analyze density, drag and altitude decay effects.",
    },
    AiPreset {
        id: "orbit_recommend",
        name: "推荐轨道与传感器参数",
        name_en: "Recommend orbit and sensor parameters",
        description: "根据任务目标推荐轨道高度、倾角与传感器视场/指向参数。",
        description_en: "Recommend altitude, inclination and sensor FOV/pointing parameters.",
    },
];

#[derive(Debug, Deserialize)]
struct ChatResponse {
    #[serde(default)]
    choices: Vec<ChatChoice>,
}

#[derive(Debug, Deserialize)]
struct ChatChoice {
    message: ChatMessage,
}

#[derive(Debug, Deserialize)]
struct ChatMessage {
    #[serde(default)]
    content: Option<String>,
}

impl AiClient {
    pub fn new(config: &Config) -> Self {
        let http = reqwest::Client::builder()
            .timeout(Duration::from_secs(config.ai_timeout_seconds.max(5)))
            .build()
            .unwrap_or_default();
        Self {
            enabled: config.ai_enabled
                && !config.ai_api_key.trim().is_empty()
                && !config.ai_model.trim().is_empty(),
            base_url: config.ai_base_url.trim().trim_end_matches('/').to_string(),
            api_key: config.ai_api_key.clone(),
            model: config.ai_model.clone(),
            http,
        }
    }

    pub fn enabled(&self) -> bool {
        self.enabled
    }

    pub fn model(&self) -> &str {
        &self.model
    }

    fn endpoint(&self) -> String {
        if self.base_url.ends_with("/chat/completions") {
            self.base_url.clone()
        } else {
            format!("{}/chat/completions", self.base_url)
        }
    }

    /// 调用 Chat Completions 并返回（正文, 耗时毫秒）。
    pub async fn complete(&self, system: &str, user: &str) -> Result<(String, i64), String> {
        if !self.enabled {
            return Err("AI 功能未启用，请在后端配置 KST_AI_API_KEY 与 KST_AI_MODEL".into());
        }
        let started = std::time::Instant::now();
        let response = self
            .http
            .post(self.endpoint())
            .bearer_auth(&self.api_key)
            .json(&serde_json::json!({
                "model": self.model,
                "messages": [
                    { "role": "system", "content": system },
                    { "role": "user", "content": user }
                ],
                "temperature": 0.3,
                "max_tokens": 1600
            }))
            .send()
            .await
            .map_err(|err| {
                if err.is_timeout() {
                    "AI 请求超时，请检查网络或调大 KST_AI_TIMEOUT_SECONDS".to_string()
                } else {
                    format!("AI 请求失败：{err}")
                }
            })?;

        let status = response.status();
        let body = response
            .text()
            .await
            .map_err(|err| format!("AI 响应读取失败：{err}"))?;
        if !status.is_success() {
            let snippet: String = body.chars().take(400).collect();
            return Err(format!("AI 服务返回 {status}：{snippet}"));
        }

        let parsed: ChatResponse =
            serde_json::from_str(&body).map_err(|err| format!("AI 响应解析失败：{err}"))?;
        let content = parsed
            .choices
            .first()
            .and_then(|choice| choice.message.content.as_deref())
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .ok_or_else(|| "AI 响应为空".to_string())?
            .to_string();

        Ok((content, started.elapsed().as_millis() as i64))
    }
}

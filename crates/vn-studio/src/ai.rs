//! Подключение ИИ: облачного (OpenAI-совместимые сервисы, Anthropic) и локального (Ollama, LM Studio).
//! Построение запроса и разбор ответа — чистые функции; сеть только в `send`.
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum Protocol {
    /// `POST {адрес}/chat/completions` — OpenAI, OpenRouter, Ollama, LM Studio, llama.cpp и другие.
    #[default]
    OpenAi,
    /// `POST {адрес}/messages` — Anthropic.
    Anthropic,
}

pub struct Preset {
    pub name: &'static str,
    pub protocol: Protocol,
    pub base_url: &'static str,
    pub local: bool,
}

pub const PRESETS: [Preset; 6] = [
    Preset { name: "Ollama (локально)", protocol: Protocol::OpenAi, base_url: "http://localhost:11434/v1", local: true },
    Preset { name: "LM Studio (локально)", protocol: Protocol::OpenAi, base_url: "http://localhost:1234/v1", local: true },
    Preset { name: "OpenAI (облако)", protocol: Protocol::OpenAi, base_url: "https://api.openai.com/v1", local: false },
    Preset { name: "OpenRouter (облако)", protocol: Protocol::OpenAi, base_url: "https://openrouter.ai/api/v1", local: false },
    Preset { name: "Anthropic (облако)", protocol: Protocol::Anthropic, base_url: "https://api.anthropic.com/v1", local: false },
    Preset { name: "Другой OpenAI-совместимый", protocol: Protocol::OpenAi, base_url: "", local: false },
];

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct AiSettings {
    pub preset: usize,
    pub protocol: Protocol,
    pub base_url: String,
    pub api_key: String,
    pub model: String,
}

impl Default for AiSettings {
    fn default() -> Self {
        Self { preset: 0, protocol: PRESETS[0].protocol, base_url: PRESETS[0].base_url.into(), api_key: String::new(), model: String::new() }
    }
}

impl AiSettings {
    pub fn use_preset(&mut self, index: usize) {
        let p = &PRESETS[index];
        self.preset = index;
        self.protocol = p.protocol;
        self.base_url = p.base_url.into();
        self.model.clear();
    }
    pub fn is_local(&self) -> bool {
        PRESETS.get(self.preset).is_some_and(|p| p.local)
    }
    /// Чего не хватает, чтобы отправить запрос.
    pub fn missing(&self) -> Option<&'static str> {
        if self.base_url.trim().is_empty() {
            Some("Укажите адрес сервера ИИ.")
        } else if self.model.trim().is_empty() {
            Some("Укажите модель (кнопка «Список моделей» покажет доступные).")
        } else if !self.is_local() && self.api_key.trim().is_empty() {
            Some("Для облачного ИИ нужен ключ API.")
        } else {
            None
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct HttpRequest {
    pub url: String,
    pub headers: Vec<(String, String)>,
    /// `None` — запрос GET.
    pub body: Option<Value>,
}

fn headers(s: &AiSettings) -> Vec<(String, String)> {
    let key = s.api_key.trim();
    match s.protocol {
        Protocol::Anthropic => vec![("x-api-key".into(), key.into()), ("anthropic-version".into(), "2023-06-01".into())],
        Protocol::OpenAi if key.is_empty() => vec![],
        Protocol::OpenAi => vec![("Authorization".into(), format!("Bearer {key}"))],
    }
}

fn endpoint(s: &AiSettings, path: &str) -> String {
    format!("{}/{path}", s.base_url.trim().trim_end_matches('/'))
}

pub fn chat_request(s: &AiSettings, system: &str, user: &str) -> HttpRequest {
    let model = s.model.trim();
    let (path, body) = match s.protocol {
        Protocol::OpenAi => ("chat/completions", json!({ "model": model, "stream": false, "messages": [{ "role": "system", "content": system }, { "role": "user", "content": user }] })),
        Protocol::Anthropic => ("messages", json!({ "model": model, "max_tokens": 8000, "system": system, "messages": [{ "role": "user", "content": user }] })),
    };
    HttpRequest { url: endpoint(s, path), headers: headers(s), body: Some(body) }
}

pub fn models_request(s: &AiSettings) -> HttpRequest {
    HttpRequest { url: endpoint(s, "models"), headers: headers(s), body: None }
}

fn api_error(status: u16, reply: &Value) -> String {
    let detail = reply["error"]["message"].as_str().or(reply["error"].as_str()).or(reply["message"].as_str()).unwrap_or("подробностей нет");
    let hint = match status {
        401 | 403 => " Проверьте ключ API.",
        404 => " Проверьте адрес сервера и название модели.",
        429 => " Превышен лимит запросов или закончился баланс.",
        _ => "",
    };
    format!("Сервер ИИ ответил ошибкой {status}: {detail}.{hint}")
}

/// Текст ответа модели.
pub fn chat_reply(protocol: Protocol, status: u16, reply: &Value) -> Result<String, String> {
    if !(200..300).contains(&status) {
        return Err(api_error(status, reply));
    }
    let text = match protocol {
        Protocol::OpenAi => reply["choices"][0]["message"]["content"].as_str().map(str::to_string),
        Protocol::Anthropic => reply["content"].as_array().map(|parts| parts.iter().filter_map(|p| p["text"].as_str()).collect::<String>()),
    };
    text.filter(|t| !t.trim().is_empty()).ok_or_else(|| "ИИ вернул пустой ответ. Попробуйте другую модель.".to_string())
}

pub fn models_reply(status: u16, reply: &Value) -> Result<Vec<String>, String> {
    if !(200..300).contains(&status) {
        return Err(api_error(status, reply));
    }
    let mut names: Vec<String> = reply["data"].as_array().map(|a| a.iter().filter_map(|m| m["id"].as_str().map(str::to_string)).collect()).unwrap_or_default();
    names.sort();
    Ok(names)
}

/// Отправить запрос. Возвращает код ответа и тело.
pub fn send(req: &HttpRequest) -> Result<(u16, Value), String> {
    let agent: ureq::Agent = ureq::Agent::config_builder().http_status_as_error(false).timeout_global(Some(std::time::Duration::from_secs(600))).build().into();
    let connect = |e: ureq::Error| format!("Не удалось связаться с {}: {e}. Для локального ИИ проверьте, что программа запущена; для облачного — интернет.", req.url);
    let mut resp = match &req.body {
        Some(body) => {
            let mut r = agent.post(&req.url);
            for (k, v) in &req.headers {
                r = r.header(k, v);
            }
            r.send_json(body).map_err(connect)?
        }
        None => {
            let mut r = agent.get(&req.url);
            for (k, v) in &req.headers {
                r = r.header(k, v);
            }
            r.call().map_err(connect)?
        }
    };
    let status = resp.status().as_u16();
    let text = resp.body_mut().read_to_string().map_err(|e| format!("Ответ сервера ИИ не прочитан: {e}"))?;
    Ok((status, serde_json::from_str(&text).unwrap_or_else(|_| json!({ "message": text.chars().take(300).collect::<String>() }))))
}

pub fn ask(s: &AiSettings, system: &str, user: &str) -> Result<String, String> {
    let (status, reply) = send(&chat_request(s, system, user))?;
    chat_reply(s.protocol, status, &reply)
}

pub fn list_models(s: &AiSettings) -> Result<Vec<String>, String> {
    let (status, reply) = send(&models_request(s))?;
    models_reply(status, &reply)
}

pub const GUIDE: &str = "\
ЛОКАЛЬНЫЙ ИИ (бесплатно, работает без интернета, текст не покидает компьютер)
 1. Установите Ollama с сайта ollama.com (или LM Studio с lmstudio.ai).
 2. Скачайте модель. Для Ollama: откройте «Командную строку» и выполните
      ollama pull <название модели>
    Название выберите в каталоге на ollama.com. Чем больше модель, тем лучше текст,
    но тем больше нужно памяти: для 8 ГБ ОЗУ берите модели до 4 ГБ.
    В LM Studio: скачайте модель во вкладке поиска и включите «Local Server».
 3. Здесь выберите «Ollama (локально)» или «LM Studio (локально)».
    Адрес подставится сам, ключ не нужен.
 4. Нажмите «Список моделей» и выберите скачанную модель.

ОБЛАЧНЫЙ ИИ (лучше качество, платно по тарифу сервиса, нужен интернет)
 1. Зарегистрируйтесь в сервисе: OpenAI, OpenRouter или Anthropic.
 2. В личном кабинете создайте ключ API и пополните баланс, если сервис этого требует.
 3. Здесь выберите сервис, вставьте ключ в поле «Ключ API».
 4. Нажмите «Список моделей» и выберите модель.
 Ключ хранится только на этом компьютере, в настройках редактора, и в проект
 или собранную игру не попадает. Текст вашей идеи отправляется выбранному сервису.

ДРУГОЙ СЕРВИС
 Подойдёт любой сервер с OpenAI-совместимым API (llama.cpp, KoboldCpp, vLLM и т. п.):
 выберите «Другой OpenAI-совместимый» и введите адрес, оканчивающийся на /v1.

ЧТО ДЕЛАЕТ ИИ
 • «Новая история» — по вашей идее создаёт сцены, реплики, выборы и концовки.
 • «Продолжить сцену» — дописывает реплики в выбранную сцену.
 Результат сначала показывается для просмотра и добавляется в проект только по кнопке.
 Всё созданное можно править вручную, а действие — отменить (Ctrl+Z).";

#[cfg(test)]
mod tests {
    use super::*;

    fn settings(preset: usize, key: &str, model: &str) -> AiSettings {
        let mut s = AiSettings::default();
        s.use_preset(preset);
        s.api_key = key.into();
        s.model = model.into();
        s
    }

    #[test]
    fn local_request_has_no_key_and_uses_chat_completions() {
        let r = chat_request(&settings(0, "", "m1"), "sys", "idea");
        assert_eq!(r.url, "http://localhost:11434/v1/chat/completions");
        assert!(r.headers.is_empty());
        let body = r.body.unwrap();
        assert_eq!(body["model"], "m1");
        assert_eq!(body["messages"][0], json!({"role": "system", "content": "sys"}));
        assert_eq!(body["messages"][1]["content"], "idea");
    }

    #[test]
    fn cloud_requests_carry_the_key_in_the_right_header() {
        let r = chat_request(&settings(2, " sk-1 ", "m"), "s", "u");
        assert_eq!(r.headers, vec![("Authorization".to_string(), "Bearer sk-1".to_string())]);
        let mut a = settings(4, "key", "m");
        a.base_url.push('/');
        let r = chat_request(&a, "s", "u");
        assert_eq!(r.url, "https://api.anthropic.com/v1/messages");
        assert!(r.headers.contains(&("x-api-key".to_string(), "key".to_string())));
        let body = r.body.unwrap();
        assert_eq!(body["system"], "s");
        assert!(body["max_tokens"].as_u64().unwrap() > 0);
    }

    #[test]
    fn replies_of_both_protocols_are_read() {
        let openai = json!({"choices": [{"message": {"content": "{\"scenes\":[]}"}}]});
        assert_eq!(chat_reply(Protocol::OpenAi, 200, &openai).unwrap(), "{\"scenes\":[]}");
        let anthropic = json!({"content": [{"type": "text", "text": "a"}, {"type": "text", "text": "b"}]});
        assert_eq!(chat_reply(Protocol::Anthropic, 200, &anthropic).unwrap(), "ab");
        assert!(chat_reply(Protocol::OpenAi, 200, &json!({"choices": []})).is_err());
    }

    #[test]
    fn errors_explain_what_to_check() {
        let e = chat_reply(Protocol::OpenAi, 401, &json!({"error": {"message": "bad key"}})).unwrap_err();
        assert!(e.contains("bad key") && e.contains("ключ"), "{e}");
        let e = models_reply(404, &json!({"error": "model not found"})).unwrap_err();
        assert!(e.contains("model not found") && e.contains("адрес"), "{e}");
    }

    #[test]
    fn models_are_listed_sorted() {
        assert_eq!(models_reply(200, &json!({"data": [{"id": "b"}, {"id": "a"}]})).unwrap(), vec!["a", "b"]);
    }

    #[test]
    fn missing_settings_are_named() {
        assert!(settings(0, "", "").missing().unwrap().contains("модель"));
        assert!(settings(0, "", "m").missing().is_none(), "локальному ИИ ключ не нужен");
        assert!(settings(2, "", "m").missing().unwrap().contains("ключ"));
        assert!(settings(5, "k", "m").missing().unwrap().contains("адрес"));
    }
}

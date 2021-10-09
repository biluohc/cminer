use crate::config::Proxy;
use std::sync::Arc;

#[derive(Debug, Clone)]
pub struct State<T: AsRef<str>> {
    pub proxy: Proxy,
    pub upstreams: Vec<Arc<T>>,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct Metric {
    pub url: String,
    pub connections: usize,
}

impl<T> State<T>
where
    T: AsRef<str>,
{
    pub fn to_metric(&self) -> Vec<Metric> {
        self.upstreams
            .iter()
            .map(|u| Metric {
                url: u.as_ref().as_ref().to_owned(),
                connections: Arc::weak_count(u),
            })
            .collect()
    }
}

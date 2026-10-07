pub mod fetch;
pub mod opml;
pub mod store;

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Subscription {
    pub id: String,
    pub url: String,
    pub title: String,
    pub folder: String,
    pub unread: i64,
    pub last_fetched: Option<String>,
    pub last_error: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Article {
    pub id: String,
    pub feed_id: String,
    pub feed_title: String,
    pub title: String,
    pub url: Option<String>,
    pub content: String,
    pub published: String,
    pub read: bool,
    pub starred: bool,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ArticleQuery {
    pub feed_id: Option<String>,
    pub folder: Option<String>,
    pub unread: Option<bool>,
    pub starred: Option<bool>,
    pub q: Option<String>,
    pub limit: Option<u32>,
    pub offset: Option<u32>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ArticlePatch {
    pub read: Option<bool>,
    pub starred: Option<bool>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NewSubscription {
    pub url: String,
    #[serde(default)]
    pub title: String,
    #[serde(default)]
    pub folder: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Stats {
    pub feeds: i64,
    pub articles: i64,
    pub unread: i64,
    pub starred: i64,
}

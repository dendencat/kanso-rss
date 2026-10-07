use crate::{
    fetch::FetchedFeed, Article, ArticlePatch, ArticleQuery, NewSubscription, Stats, Subscription,
};
use anyhow::{bail, Context, Result};
use rusqlite::{params, Connection, OptionalExtension};
use std::path::Path;

pub struct Store {
    connection: Connection,
}

impl Store {
    pub fn open(path: impl AsRef<Path>) -> Result<Self> {
        let connection = Connection::open(path)?;
        Self::from_connection(connection)
    }

    pub fn memory() -> Result<Self> {
        Self::from_connection(Connection::open_in_memory()?)
    }

    pub fn backup(&self, path: &Path) -> Result<()> {
        let mut options = std::fs::OpenOptions::new();
        options.create_new(true).write(true);
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt;
            options.mode(0o600);
        }
        let file = options
            .open(path)
            .context("Backup destination must be new")?;
        let result = (|| {
            self.connection.backup("main", path, None)?;
            let check =
                Connection::open_with_flags(path, rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY)?;
            let integrity: String =
                check.pragma_query_value(None, "integrity_check", |r| r.get(0))?;
            if integrity != "ok" {
                bail!("Backup integrity check failed");
            }
            file.sync_all()?;
            Ok(())
        })();
        if result.is_err() {
            let _ = std::fs::remove_file(path);
        }
        result
    }

    fn from_connection(connection: Connection) -> Result<Self> {
        let version: i64 = connection.pragma_query_value(None, "user_version", |r| r.get(0))?;
        if version > 1 {
            bail!("Database schema is newer than this application; refusing to downgrade");
        }
        connection.busy_timeout(std::time::Duration::from_secs(5))?;
        let needs_search_index = connection.query_row(
            "SELECT count(*) FROM sqlite_master WHERE name='articles_fts'",
            [],
            |r| r.get::<_, i64>(0),
        )? == 0;
        connection.execute_batch(
            "PRAGMA foreign_keys=ON; PRAGMA journal_mode=WAL;
            BEGIN IMMEDIATE;
            CREATE TABLE IF NOT EXISTS feeds (
                id TEXT PRIMARY KEY, url TEXT NOT NULL UNIQUE, title TEXT NOT NULL,
                folder TEXT NOT NULL DEFAULT '', last_fetched TEXT, last_error TEXT);
            CREATE TABLE IF NOT EXISTS articles (
                id TEXT PRIMARY KEY, feed_id TEXT NOT NULL REFERENCES feeds(id) ON DELETE CASCADE,
                entry_key TEXT NOT NULL, title TEXT NOT NULL, url TEXT, content TEXT NOT NULL,
                published TEXT NOT NULL, read INTEGER NOT NULL DEFAULT 0,
                starred INTEGER NOT NULL DEFAULT 0, UNIQUE(feed_id,entry_key));
            CREATE INDEX IF NOT EXISTS articles_feed_date ON articles(feed_id,published DESC);
            CREATE INDEX IF NOT EXISTS articles_date ON articles(published DESC);
            CREATE VIRTUAL TABLE IF NOT EXISTS articles_fts USING fts5(title,content,content='articles',content_rowid='rowid',tokenize='trigram');
            CREATE TRIGGER IF NOT EXISTS articles_search_insert AFTER INSERT ON articles BEGIN
                INSERT INTO articles_fts(rowid,title,content) VALUES(new.rowid,new.title,new.content); END;
            CREATE TRIGGER IF NOT EXISTS articles_search_delete AFTER DELETE ON articles BEGIN
                INSERT INTO articles_fts(articles_fts,rowid,title,content) VALUES('delete',old.rowid,old.title,old.content); END;
            CREATE TRIGGER IF NOT EXISTS articles_search_update AFTER UPDATE OF title,content ON articles BEGIN
                INSERT INTO articles_fts(articles_fts,rowid,title,content) VALUES('delete',old.rowid,old.title,old.content);
                INSERT INTO articles_fts(rowid,title,content) VALUES(new.rowid,new.title,new.content); END;
            PRAGMA user_version=1;",
        )?;
        if needs_search_index {
            connection
                .execute_batch("INSERT INTO articles_fts(articles_fts) VALUES('rebuild');")?;
        }
        connection.execute_batch("COMMIT;")?;
        Ok(Self { connection })
    }

    pub fn add_feed(&self, value: &NewSubscription) -> Result<Subscription> {
        let url = crate::fetch::validate_url(&value.url)?.to_string();
        if value.title.chars().count() > 256 || value.folder.chars().count() > 128 {
            bail!("Title or folder is too long");
        }
        if let Some(id) = self
            .connection
            .query_row("SELECT id FROM feeds WHERE url=?1", [&url], |r| {
                r.get::<_, String>(0)
            })
            .optional()?
        {
            return self.feed(&id)?.context("Feed disappeared");
        }
        if self
            .connection
            .query_row("SELECT count(*) FROM feeds", [], |r| r.get::<_, i64>(0))?
            >= 500
        {
            bail!("Subscription limit (500) reached");
        }
        let id = uuid::Uuid::new_v4().to_string();
        let title = if value.title.trim().is_empty() {
            crate::fetch::validate_url(&url)?
                .host_str()
                .unwrap_or("Feed")
                .to_owned()
        } else {
            value.title.clone()
        };
        self.connection.execute(
            "INSERT INTO feeds(id,url,title,folder) VALUES(?1,?2,?3,?4)",
            params![id, url, title, value.folder],
        )?;
        self.feed(&id)?.context("Feed disappeared")
    }

    pub fn feeds(&self) -> Result<Vec<Subscription>> {
        let mut statement = self.connection.prepare("SELECT f.id,f.url,f.title,f.folder,
            (SELECT count(*) FROM articles a WHERE a.feed_id=f.id AND a.read=0),f.last_fetched,f.last_error
            FROM feeds f ORDER BY f.folder COLLATE NOCASE,f.title COLLATE NOCASE")?;
        let result = statement
            .query_map([], |r| {
                Ok(Subscription {
                    id: r.get(0)?,
                    url: r.get(1)?,
                    title: r.get(2)?,
                    folder: r.get(3)?,
                    unread: r.get(4)?,
                    last_fetched: r.get(5)?,
                    last_error: r.get(6)?,
                })
            })?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        Ok(result)
    }

    pub fn feed(&self, id: &str) -> Result<Option<Subscription>> {
        Ok(self.feeds()?.into_iter().find(|f| f.id == id))
    }

    pub fn update_feed(&self, id: &str, title: &str, folder: &str) -> Result<bool> {
        if title.trim().is_empty() || title.chars().count() > 256 || folder.chars().count() > 128 {
            bail!("Invalid title or folder");
        }
        Ok(self.connection.execute(
            "UPDATE feeds SET title=?1,folder=?2 WHERE id=?3",
            params![title, folder, id],
        )? > 0)
    }

    pub fn delete_feed(&self, id: &str) -> Result<bool> {
        Ok(self
            .connection
            .execute("DELETE FROM feeds WHERE id=?1", [id])?
            > 0)
    }

    pub fn ingest(&mut self, id: &str, fetched: FetchedFeed) -> Result<usize> {
        let tx = self.connection.transaction()?;
        let exists = tx
            .query_row("SELECT 1 FROM feeds WHERE id=?1", [id], |_| Ok(()))
            .optional()?
            .is_some();
        if !exists {
            bail!("Feed not found");
        }
        let count: i64 = tx.query_row("SELECT count(*) FROM articles", [], |r| r.get(0))?;
        let mut available = (100_000 - count).max(0);
        let mut added = 0;
        for article in fetched.articles {
            if available == 0 {
                let existing = tx
                    .query_row(
                        "SELECT 1 FROM articles WHERE feed_id=?1 AND entry_key=?2",
                        params![id, article.key],
                        |_| Ok(()),
                    )
                    .optional()?
                    .is_some();
                if existing {
                    continue;
                }
                bail!("Article capacity (100,000) reached; remove unused subscriptions");
            }
            let changed = tx.execute(
                "INSERT OR IGNORE INTO articles(id,feed_id,entry_key,title,url,content,published)
                VALUES(?1,?2,?3,?4,?5,?6,?7)",
                params![
                    uuid::Uuid::new_v4().to_string(),
                    id,
                    article.key,
                    article.title,
                    article.url,
                    article.content,
                    article.published
                ],
            )?;
            added += changed;
            available -= changed as i64;
        }
        tx.execute(
            "UPDATE feeds SET last_fetched=?1,last_error=NULL WHERE id=?2",
            params![chrono::Utc::now().to_rfc3339(), id],
        )?;
        tx.execute("DELETE FROM articles WHERE feed_id=?1 AND starred=0 AND id IN
            (SELECT id FROM articles WHERE feed_id=?1 ORDER BY published DESC,id DESC LIMIT -1 OFFSET 5000)", [id])?;
        tx.commit()?;
        Ok(added)
    }

    pub fn fetch_error(&self, id: &str, error: &str) -> Result<()> {
        let error: String = error.chars().take(300).collect();
        self.connection.execute(
            "UPDATE feeds SET last_error=?1 WHERE id=?2",
            params![error, id],
        )?;
        Ok(())
    }

    pub fn articles(&self, query: &ArticleQuery) -> Result<Vec<Article>> {
        if query.q.as_ref().is_some_and(|q| q.len() > 1024) {
            bail!("Search is too long");
        }
        let pattern = query.q.as_ref().filter(|q| !q.is_empty()).map(|q| {
            format!(
                "%{}%",
                q.replace('\\', "\\\\")
                    .replace('%', "\\%")
                    .replace('_', "\\_")
            )
        });
        let full_text = query
            .q
            .as_ref()
            .filter(|q| q.chars().count() >= 3)
            .map(|q| format!("\"{}\"", q.replace('"', "\"\"")));
        let mut statement = self.connection.prepare("SELECT a.id,a.feed_id,f.title,a.title,a.url,substr(a.content,1,400),a.published,a.read,a.starred
            FROM articles a JOIN feeds f ON a.feed_id=f.id
            WHERE (?1 IS NULL OR a.feed_id=?1) AND (?2 IS NULL OR f.folder=?2)
            AND (?3 IS NULL OR a.read=NOT ?3) AND (?4 IS NULL OR a.starred=?4)
            AND (?5 IS NULL OR (?8 IS NOT NULL AND a.rowid IN (SELECT rowid FROM articles_fts WHERE articles_fts MATCH ?8))
                OR (?8 IS NULL AND (a.title LIKE ?5 ESCAPE '\\' OR a.content LIKE ?5 ESCAPE '\\')))
            ORDER BY a.published DESC,a.id DESC LIMIT ?6 OFFSET ?7")?;
        let result = statement
            .query_map(
                params![
                    query.feed_id,
                    query.folder,
                    query.unread,
                    query.starred,
                    pattern,
                    query.limit.unwrap_or(50).clamp(1, 200),
                    query.offset.unwrap_or(0).min(100_000),
                    full_text
                ],
                |r| {
                    Ok(Article {
                        id: r.get(0)?,
                        feed_id: r.get(1)?,
                        feed_title: r.get(2)?,
                        title: r.get(3)?,
                        url: r.get(4)?,
                        content: r.get(5)?,
                        published: r.get(6)?,
                        read: r.get(7)?,
                        starred: r.get(8)?,
                    })
                },
            )?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        Ok(result)
    }

    pub fn article(&self, id: &str) -> Result<Option<Article>> {
        Ok(self
            .connection
            .query_row(
                "SELECT a.id,a.feed_id,f.title,a.title,a.url,a.content,a.published,a.read,a.starred
            FROM articles a JOIN feeds f ON a.feed_id=f.id WHERE a.id=?1",
                [id],
                |r| {
                    Ok(Article {
                        id: r.get(0)?,
                        feed_id: r.get(1)?,
                        feed_title: r.get(2)?,
                        title: r.get(3)?,
                        url: r.get(4)?,
                        content: r.get(5)?,
                        published: r.get(6)?,
                        read: r.get(7)?,
                        starred: r.get(8)?,
                    })
                },
            )
            .optional()?)
    }

    pub fn patch_article(&self, id: &str, patch: &ArticlePatch) -> Result<bool> {
        Ok(self.connection.execute(
            "UPDATE articles SET read=COALESCE(?1,read),starred=COALESCE(?2,starred) WHERE id=?3",
            params![patch.read, patch.starred, id],
        )? > 0)
    }

    pub fn mark_read(&self, feed_id: Option<&str>, folder: Option<&str>) -> Result<usize> {
        Ok(self.connection.execute(
            "UPDATE articles SET read=1 WHERE read=0 AND (?1 IS NULL OR feed_id=?1)
            AND (?2 IS NULL OR feed_id IN (SELECT id FROM feeds WHERE folder=?2))",
            params![feed_id, folder],
        )?)
    }

    pub fn stats(&self) -> Result<Stats> {
        Ok(self.connection.query_row("SELECT (SELECT count(*) FROM feeds),count(*),COALESCE(sum(read=0),0),COALESCE(sum(starred=1),0) FROM articles", [], |r| Ok(Stats { feeds:r.get(0)?,articles:r.get(1)?,unread:r.get(2)?,starred:r.get(3)? }))?)
    }

    pub fn import(&mut self, subscriptions: &[NewSubscription]) -> Result<usize> {
        // Validate the entire document before changing the database. A savepoint
        // also rolls back capacity/constraint failures part-way through import.
        for s in subscriptions {
            crate::fetch::validate_url(&s.url)?;
        }
        self.connection.execute_batch("SAVEPOINT opml_import")?;
        let result = (|| {
            let before = self.stats()?.feeds;
            for s in subscriptions {
                self.add_feed(s)?;
            }
            Ok((self.stats()?.feeds - before) as usize)
        })();
        match result {
            Ok(n) => {
                self.connection.execute_batch("RELEASE opml_import")?;
                Ok(n)
            }
            Err(e) => {
                self.connection
                    .execute_batch("ROLLBACK TO opml_import; RELEASE opml_import")?;
                Err(e)
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn rss() -> FetchedFeed {
        crate::fetch::parse(br#"<rss version="2.0"><channel><title>Test</title><link>https://example.org</link><description>x</description><item><guid>a</guid><title>A 100% story</title><link>https://example.org/a</link><description>Hello</description></item></channel></rss>"#).unwrap()
    }
    #[test]
    fn ingestion_is_idempotent_and_preserves_state() {
        let mut store = Store::memory().unwrap();
        let f = store
            .add_feed(&NewSubscription {
                url: "https://example.org/rss".into(),
                title: "Test".into(),
                folder: "Tech".into(),
            })
            .unwrap();
        assert_eq!(store.ingest(&f.id, rss()).unwrap(), 1);
        let a = store.articles(&ArticleQuery::default()).unwrap().remove(0);
        store
            .patch_article(
                &a.id,
                &ArticlePatch {
                    read: Some(true),
                    starred: Some(true),
                },
            )
            .unwrap();
        assert_eq!(store.ingest(&f.id, rss()).unwrap(), 0);
        assert_eq!(store.stats().unwrap().unread, 0);
        assert_eq!(store.stats().unwrap().starred, 1);
        assert_eq!(
            store
                .articles(&ArticleQuery {
                    q: Some("100%".into()),
                    ..Default::default()
                })
                .unwrap()
                .len(),
            1
        );
        assert!(store
            .articles(&ArticleQuery {
                q: Some("' OR 1=1 --".into()),
                ..Default::default()
            })
            .unwrap()
            .is_empty());
        assert!(store.delete_feed(&f.id).unwrap());
        assert_eq!(store.stats().unwrap().articles, 0);
    }
    #[test]
    fn import_rolls_back_invalid_document() {
        let mut s = Store::memory().unwrap();
        let feeds = [
            NewSubscription {
                url: "https://example.org/feed".into(),
                title: "ok".into(),
                folder: "".into(),
            },
            NewSubscription {
                url: "https://example.net/feed".into(),
                title: "x".repeat(257),
                folder: "".into(),
            },
        ];
        assert!(s.import(&feeds).is_err());
        assert_eq!(s.stats().unwrap().feeds, 0);
    }
    #[test]
    fn persists_after_reopening() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("test.sqlite");
        Store::open(&path)
            .unwrap()
            .add_feed(&NewSubscription {
                url: "https://example.org/rss".into(),
                title: "Test".into(),
                folder: "Tech".into(),
            })
            .unwrap();
        assert_eq!(Store::open(path).unwrap().stats().unwrap().feeds, 1);
    }
    #[test]
    fn live_backup_is_consistent_and_does_not_overwrite() {
        let dir = tempfile::tempdir().unwrap();
        let store = Store::open(dir.path().join("live.sqlite")).unwrap();
        store
            .add_feed(&NewSubscription {
                url: "https://example.org/rss".into(),
                title: "Test".into(),
                folder: "".into(),
            })
            .unwrap();
        let backup = dir.path().join("backup.sqlite");
        store.backup(&backup).unwrap();
        assert_eq!(Store::open(&backup).unwrap().stats().unwrap().feeds, 1);
        assert!(store.backup(&backup).is_err());
    }
    #[test]
    fn japanese_substring_search_and_index_deletion() {
        let mut store = Store::memory().unwrap();
        let f = store
            .add_feed(&NewSubscription {
                url: "https://example.org/rss".into(),
                title: "Test".into(),
                folder: "".into(),
            })
            .unwrap();
        let mut fetched = rss();
        fetched.articles[0].title = "Rustで考える記事検索の安全性".into();
        store.ingest(&f.id, fetched).unwrap();
        let query = ArticleQuery {
            q: Some("検索の安全".into()),
            ..Default::default()
        };
        assert_eq!(store.articles(&query).unwrap().len(), 1);
        let punctuation = ArticleQuery {
            q: Some("\" OR *".into()),
            ..Default::default()
        };
        assert!(store.articles(&punctuation).unwrap().is_empty());
        store.delete_feed(&f.id).unwrap();
        assert!(store.articles(&query).unwrap().is_empty());
    }
}

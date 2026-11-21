use crate::{
    quota::{ParsedQuota, QuotaSample},
    settings::Preferences,
};
use rusqlite::{params, Connection, OptionalExtension};
use serde::Serialize;
use std::{path::Path, time::Duration};

pub struct Store {
    connection: Connection,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct HistoryPoint {
    #[serde(flatten)]
    pub sample: QuotaSample,
    pub segment: String,
}

impl Store {
    pub fn open(path: &Path) -> rusqlite::Result<Self> {
        Self::initialize(Connection::open(path)?)
    }
    pub fn memory() -> rusqlite::Result<Self> {
        Self::initialize(Connection::open_in_memory()?)
    }

    fn initialize(connection: Connection) -> rusqlite::Result<Self> {
        connection.busy_timeout(Duration::from_secs(2))?;
        connection.execute_batch(
            "PRAGMA journal_mode=WAL;
             CREATE TABLE IF NOT EXISTS samples (
               account_key TEXT NOT NULL, recorded_at INTEGER NOT NULL,
               remaining_percent REAL NOT NULL CHECK(remaining_percent BETWEEN 0 AND 100),
               resets_at INTEGER, window_minutes INTEGER NOT NULL,
               segment_start INTEGER NOT NULL, poll_interval_secs INTEGER NOT NULL,
               PRIMARY KEY(account_key, recorded_at)
             );
             CREATE INDEX IF NOT EXISTS samples_time ON samples(recorded_at);
             CREATE TABLE IF NOT EXISTS source_latest (
               source_key TEXT PRIMARY KEY NOT NULL, account_key TEXT NOT NULL,
               recorded_at INTEGER NOT NULL, remaining_percent REAL NOT NULL,
               resets_at INTEGER, window_minutes INTEGER NOT NULL
             );
             CREATE TABLE IF NOT EXISTS personal_account_links (
               email_key TEXT NOT NULL, account_key TEXT NOT NULL,
               PRIMARY KEY(email_key, account_key)
             );
             CREATE TABLE IF NOT EXISTS preferences (id INTEGER PRIMARY KEY CHECK(id=1), value TEXT NOT NULL);"
        )?;
        Ok(Self { connection })
    }

    pub fn record(&self, quota: &ParsedQuota) -> rusqlite::Result<()> {
        self.record_with_boundary(quota, false)
    }

    pub fn record_with_boundary(
        &self,
        quota: &ParsedQuota,
        disconnected: bool,
    ) -> rusqlite::Result<()> {
        let disconnected = disconnected
            || self
                .latest()?
                .is_some_and(|last| last.account_key != quota.account_key);
        let previous = self.connection.query_row(
            "SELECT recorded_at,remaining_percent,resets_at,window_minutes,segment_start,poll_interval_secs
             FROM samples WHERE account_key=?1 AND recorded_at<=?2 ORDER BY recorded_at DESC LIMIT 1",
            params![quota.account_key,quota.sample.recorded_at],
            |row| Ok((sample_row(row,0)?,row.get::<_,i64>(4)?,row.get::<_,i64>(5)?)),
        ).optional()?;
        let current = &quota.sample;
        let segment_start = match previous {
            Some((ref sample, start, interval))
                if !disconnected
                    && current.resets_at == sample.resets_at
                    && current.recorded_at - sample.recorded_at <= (interval * 3).max(180)
                    && !(current.resets_at.is_none()
                        && current.remaining_percent > sample.remaining_percent) =>
            {
                start
            }
            _ => current.recorded_at,
        };
        let interval = self.preferences()?.settings.poll_interval_secs as i64;
        self.connection.execute(
            "INSERT INTO samples(account_key,recorded_at,remaining_percent,resets_at,window_minutes,segment_start,poll_interval_secs)
             VALUES(?1,?2,?3,?4,?5,?6,?7) ON CONFLICT(account_key,recorded_at) DO UPDATE SET
             remaining_percent=excluded.remaining_percent,resets_at=excluded.resets_at,window_minutes=excluded.window_minutes,
             segment_start=excluded.segment_start,poll_interval_secs=excluded.poll_interval_secs",
            params![quota.account_key,quota.sample.recorded_at,quota.sample.remaining_percent,quota.sample.resets_at,quota.sample.window_minutes,segment_start,interval],
        )?;
        Ok(())
    }

    pub fn latest(&self) -> rusqlite::Result<Option<ParsedQuota>> {
        self.connection.query_row(
            "SELECT account_key,recorded_at,remaining_percent,resets_at,window_minutes FROM samples ORDER BY recorded_at DESC,rowid DESC LIMIT 1",
            [], quota_row,
        ).optional()
    }

    /// 曲线按账号共享，离线回显必须来自该实例最后一次成功的采样。
    pub fn record_for_source(
        &self,
        source: &str,
        quota: &ParsedQuota,
        disconnected: bool,
    ) -> rusqlite::Result<()> {
        let transaction = self.connection.unchecked_transaction()?;
        self.record_with_boundary(quota, disconnected)?;
        if let Some(email_key) = &quota.personal_email_key {
            transaction.execute(
                "INSERT INTO personal_account_links(email_key,account_key) VALUES(?1,?2) ON CONFLICT DO NOTHING",
                params![email_key, quota.account_key],
            )?;
        }
        let sample = &quota.sample;
        transaction.execute(
            "INSERT INTO source_latest(source_key,account_key,recorded_at,remaining_percent,resets_at,window_minutes)
             VALUES(?1,?2,?3,?4,?5,?6) ON CONFLICT(source_key) DO UPDATE SET account_key=excluded.account_key,
             recorded_at=excluded.recorded_at,remaining_percent=excluded.remaining_percent,
             resets_at=excluded.resets_at,window_minutes=excluded.window_minutes",
            params![source, quota.account_key, sample.recorded_at, sample.remaining_percent, sample.resets_at, sample.window_minutes],
        )?;
        transaction.commit()
    }

    pub fn latest_for_source(&self, source: &str) -> rusqlite::Result<Option<ParsedQuota>> {
        self.connection.query_row(
            "SELECT account_key,recorded_at,remaining_percent,resets_at,window_minutes FROM source_latest WHERE source_key=?1",
            [source], quota_row,
        ).optional()
    }

    pub fn history(
        &self,
        account: &str,
        since: i64,
        until: i64,
        max_points: usize,
    ) -> rusqlite::Result<Vec<HistoryPoint>> {
        // 同一邮箱关联了不同账号 ID 时保留原始记录各自的归属，避免歧义导致误合并。
        let mut statement = self.connection.prepare(
            "WITH confirmed_links AS (
               SELECT email_key, MIN(account_key) AS account_key FROM personal_account_links
               GROUP BY email_key HAVING COUNT(*)=1
             ), current_account AS (
               SELECT COALESCE((SELECT account_key FROM confirmed_links WHERE email_key=?1), ?1) AS key
             )
             SELECT recorded_at,remaining_percent,resets_at,window_minutes,segment_start FROM samples
             WHERE account_key IN (
               SELECT key FROM current_account UNION
               SELECT email_key FROM confirmed_links WHERE account_key=(SELECT key FROM current_account)
             ) AND recorded_at>=?2 AND recorded_at<=?3
             ORDER BY recorded_at,(account_key=(SELECT key FROM current_account)) DESC,rowid DESC",
        )?;
        let mut points = statement
            .query_map(params![account, since, until], |row| {
                Ok(HistoryPoint {
                    sample: sample_row(row, 0)?,
                    segment: row.get::<_, i64>(4)?.to_string(),
                })
            })?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        points.dedup_by_key(|point| point.sample.recorded_at);
        // 分段在采集时持久化；修改设置或缩放图表不会重新解释历史断线。
        Ok(downsample(points, max_points))
    }

    pub fn prune(&self, before: i64) -> rusqlite::Result<()> {
        let transaction = self.connection.unchecked_transaction()?;
        transaction.execute("DELETE FROM samples WHERE recorded_at<?1", [before])?;
        transaction.execute("DELETE FROM source_latest WHERE recorded_at<?1", [before])?;
        transaction.commit()
    }

    pub fn preferences(&self) -> rusqlite::Result<Preferences> {
        let text: Option<String> = self
            .connection
            .query_row("SELECT value FROM preferences WHERE id=1", [], |row| {
                row.get(0)
            })
            .optional()?;
        text.map(|text| {
            serde_json::from_str(&text).map_err(|error| {
                rusqlite::Error::FromSqlConversionFailure(
                    0,
                    rusqlite::types::Type::Text,
                    Box::new(error),
                )
            })
        })
        .unwrap_or_else(|| Ok(Preferences::default()))
    }

    pub fn save_preferences(&self, preferences: &Preferences) -> rusqlite::Result<()> {
        let text = serde_json::to_string(preferences)
            .map_err(|error| rusqlite::Error::ToSqlConversionFailure(Box::new(error)))?;
        self.connection.execute("INSERT INTO preferences(id,value) VALUES(1,?1) ON CONFLICT(id) DO UPDATE SET value=excluded.value", [text])?;
        Ok(())
    }
}

fn quota_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<ParsedQuota> {
    Ok(ParsedQuota {
        account_key: row.get(0)?,
        personal_email_key: None,
        sample: sample_row(row, 1)?,
    })
}

fn sample_row(row: &rusqlite::Row<'_>, offset: usize) -> rusqlite::Result<QuotaSample> {
    Ok(QuotaSample {
        recorded_at: row.get(offset)?,
        remaining_percent: row.get(offset + 1)?,
        resets_at: row.get(offset + 2)?,
        window_minutes: row.get(offset + 3)?,
    })
}

fn downsample(points: Vec<HistoryPoint>, max_points: usize) -> Vec<HistoryPoint> {
    let max_points = max_points.clamp(4, 4096);
    if points.len() <= max_points {
        return points;
    }
    // 保留每桶首尾与极值，避免简单等距抽样掩盖一次突发消耗。
    let chunk_size = points.len().div_ceil(max_points / 4);
    let mut selected = Vec::with_capacity(max_points);
    for chunk in points.chunks(chunk_size) {
        let min = chunk
            .iter()
            .enumerate()
            .min_by(|(_, a), (_, b)| {
                a.sample
                    .remaining_percent
                    .total_cmp(&b.sample.remaining_percent)
            })
            .unwrap()
            .0;
        let max = chunk
            .iter()
            .enumerate()
            .max_by(|(_, a), (_, b)| {
                a.sample
                    .remaining_percent
                    .total_cmp(&b.sample.remaining_percent)
            })
            .unwrap()
            .0;
        let mut indices = vec![0, min, max, chunk.len() - 1];
        indices.sort_unstable();
        indices.dedup();
        selected.extend(indices.into_iter().map(|index| chunk[index].clone()));
    }
    selected
}

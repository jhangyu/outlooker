use std::path::PathBuf;
use std::sync::{Arc, RwLock};

use deadpool_postgres::{Manager, ManagerConfig, Object, Pool, RecyclingMethod};
use tokio_postgres::NoTls;

use super::credentials::Credentials;
use super::{base_config, connect, connect_admin, pool_err, DB_NAME};
use crate::dto::error::AppError;

fn pool(port: u16, user: &str, password: &str, max: usize) -> Result<Pool, AppError> {
    let mgr = Manager::from_config(base_config(port, user, password, DB_NAME), NoTls, ManagerConfig { recycling_method: RecyclingMethod::Fast });
    Pool::builder(mgr).max_size(max).build().map_err(|e| AppError::internal(format!("pool {user}: {e}")))
}

#[derive(Clone)]
pub struct Pools {
    pub app: Pool,
    pub ro: Pool,
}

impl Pools {
    /// App pool ≤ 8, read-only pool ≤ 2 (spec Part 04 §6.6 `max_connections` budget).
    pub fn build(port: u16, creds: &Credentials) -> Result<Pools, AppError> {
        Ok(Pools { app: pool(port, "outlooker_app", &creds.app, 8)?, ro: pool(port, "outlooker_ro", &creds.ro, 2)? })
    }
    pub fn close(&self) {
        self.app.close();
        self.ro.close();
    }
}

#[derive(Clone)]
pub struct DbConn {
    pub port: u16,
    pub creds: Credentials,
    pub pools: Pools,
    pub data_root: PathBuf,
}

/// Shared handle to the running cluster. The Supervisor sets it on Ready and clears it before any stop,
/// so services never hold a stale port after a PostgreSQL restart.
#[derive(Clone, Default)]
pub struct Db {
    inner: Arc<RwLock<Option<DbConn>>>,
}

impl Db {
    pub fn new() -> Db {
        Db::default()
    }
    pub fn set(&self, conn: Option<DbConn>) {
        let old = std::mem::replace(&mut *self.inner.write().unwrap_or_else(|p| p.into_inner()), conn);
        if let Some(old) = old {
            old.pools.close();
        }
    }
    pub fn conn(&self) -> Result<DbConn, AppError> {
        self.inner.read().unwrap_or_else(|p| p.into_inner()).clone().ok_or(AppError::DbNotReady)
    }
    pub fn app_pool(&self) -> Result<Pool, AppError> {
        Ok(self.conn()?.pools.app)
    }
    /// A pooled outlooker_app connection; errors mapped by `pool_err`.
    pub async fn app_client(&self) -> Result<Object, AppError> {
        self.conn()?.pools.app.get().await.map_err(pool_err)
    }
    /// A pooled outlooker_ro connection; errors mapped by `pool_err`.
    pub async fn ro_client(&self) -> Result<Object, AppError> {
        self.conn()?.pools.ro.get().await.map_err(pool_err)
    }
    pub async fn connect_admin(&self) -> Result<tokio_postgres::Client, AppError> {
        let c = self.conn()?;
        connect_admin(c.port, &c.creds).await
    }
    pub async fn connect_app(&self) -> Result<tokio_postgres::Client, AppError> {
        let c = self.conn()?;
        connect(c.port, "outlooker_app", &c.creds.app, DB_NAME).await
    }
    pub fn data_root(&self) -> Result<PathBuf, AppError> {
        Ok(self.conn()?.data_root)
    }
}

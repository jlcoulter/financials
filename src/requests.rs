use serde::Deserialize;
use uuid::Uuid;

// ── Portfolio ──

#[derive(Deserialize)]
pub struct AddItemForm {
    pub name: String,
    pub item_type: String,
}

#[derive(Deserialize)]
pub struct CreatePortfolioForm {
    pub name: String,
}

#[derive(Deserialize)]
pub struct RenamePortfolioForm {
    pub name: String,
}

#[derive(Deserialize)]
pub struct MoveItemQuery {
    pub item_id: Uuid,
    pub direction: String,
}

#[derive(Deserialize, Default)]
pub struct PortfolioQuery {
    pub flash: Option<String>,
    pub flash_type: Option<String>,
}

#[derive(Deserialize)]
pub struct CellQuery {
    pub item_id: String,
    pub date: String,
}

#[derive(Deserialize)]
pub struct DateQuery {
    pub date: String,
}

#[derive(Deserialize)]
pub struct ChangeTypeForm {
    pub item_id: Uuid,
    pub item_type: String,
}

#[derive(Deserialize)]
pub struct DeleteItemForm {
    pub item_id: Uuid,
}

// ── Reconcile ──

#[derive(Deserialize)]
pub struct CreateSessionForm {
    pub name: String,
}

#[derive(Deserialize)]
pub struct SortQuery {
    pub sort: Option<crate::models::reconcile::SortOrder>,
    pub scroll_to: Option<usize>,
}

#[derive(Deserialize)]
pub struct AddTxnForm {
    pub date: String,
    pub amount: String,
    pub vendor: Option<String>,
}

#[derive(Deserialize)]
pub struct UnlinkForm {
    pub outgoing_id: String,
}

#[derive(Deserialize)]
pub struct UnlinkReconciledForm {
    pub reconciled_id: String,
}

#[derive(Deserialize)]
pub struct RenameSessionForm {
    pub name: String,
}

// ── Settings / Backup ──

#[derive(Deserialize)]
pub struct SettingsFlash {
    pub flash: Option<String>,
}

#[derive(Deserialize, Default)]
pub struct DateRangeQuery {
    pub range: Option<String>,
}

#[derive(Deserialize)]
pub struct BackupForm {
    pub provider: String,
    pub bucket: String,
    pub path: String,
    pub region: String,
    pub endpoint: Option<String>,
    pub access_key_id: Option<String>,
    pub secret_access_key: Option<String>,
    pub b2_bucket: Option<String>,
    pub b2_path: Option<String>,
    pub b2_region: Option<String>,
    pub b2_key_id: Option<String>,
    pub b2_application_key: Option<String>,
    pub b2_endpoint: Option<String>,
    pub interval_minutes: Option<i64>,
    pub max_snapshots: Option<i64>,
}

#[derive(Deserialize)]
pub struct RestoreForm {
    pub timestamp: Option<String>,
}

/// A backup configuration form submitted to the public backup page.
/// This is needed because when the DB is lost, there's no stored config.
#[derive(Deserialize)]
pub struct PublicBackupForm {
    pub provider: String,
    pub bucket: String,
    pub path: String,
    pub region: Option<String>,
    pub endpoint: Option<String>,
    pub access_key_id: Option<String>,
    pub secret_access_key: Option<String>,
    pub b2_region: Option<String>,
    pub b2_key_id: Option<String>,
    pub b2_application_key: Option<String>,
    pub b2_endpoint: Option<String>,
    pub interval_minutes: Option<i64>,
    pub max_snapshots: Option<i64>,
}

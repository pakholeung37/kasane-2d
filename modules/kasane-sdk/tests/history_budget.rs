use kasane_core::{Canvas, Vec2};
use kasane_sdk::{prepare_png_asset, rectangle_mesh, AuthoringSession, HistoryLimits};

const DOCUMENT_ID: &str = "00000000-0000-4000-8000-000000000201";
const ASSET_ID: &str = "00000000-0000-4000-8000-000000000202";
const MESH_ID: &str = "00000000-0000-4000-8000-000000000203";

fn session(limits: HistoryLimits) -> AuthoringSession {
    AuthoringSession::with_history_limits(
        DOCUMENT_ID,
        Canvas::new(100.0, 100.0, Vec2::new(50.0, 50.0), 10.0),
        limits,
    )
    .unwrap()
}

fn asset() -> kasane_core::ImageAsset {
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures/asymmetric-2x2.png");
    prepare_png_asset(ASSET_ID, "fixture", &path).unwrap()
}

#[test]
fn oversized_commit_fails_before_publication() {
    let baseline = session(HistoryLimits::default()).estimated_content_bytes();
    let limits = HistoryLimits {
        max_steps: 50,
        max_bytes: baseline * 2 + 64,
    };
    let mut sdk = session(limits);
    let before = sdk.version();
    let asset = asset();
    let mesh = rectangle_mesh(
        MESH_ID,
        "mesh",
        ASSET_ID,
        Vec2::new(0.0, 0.0),
        Vec2::new(10.0, 10.0),
    )
    .unwrap();
    let mut edit = sdk.begin_edit("too large", None).unwrap();
    edit.create_asset(asset).unwrap();
    edit.create_mesh(mesh).unwrap();
    assert_eq!(
        edit.commit().unwrap_err().code.as_ref(),
        "HISTORY_LIMIT_EXCEEDED"
    );
    assert_eq!(sdk.version(), before);
    assert!(sdk.asset_ids().is_empty());
    assert!(sdk.mesh_ids().is_empty());
    assert_eq!(sdk.history_lengths(), (0, 0));
    assert!(sdk.drain_events().is_empty());
}

#[test]
fn successful_commit_evicts_oldest_entry_only_after_publish() {
    let limits = HistoryLimits {
        max_steps: 2,
        max_bytes: 256 * 1024 * 1024,
    };
    let mut sdk = session(limits);
    sdk.edit("asset", None, |edit| edit.create_asset(asset()))
        .unwrap();
    let mesh = rectangle_mesh(
        MESH_ID,
        "mesh",
        ASSET_ID,
        Vec2::new(0.0, 0.0),
        Vec2::new(10.0, 10.0),
    )
    .unwrap();
    sdk.edit("mesh", None, |edit| edit.create_mesh(mesh))
        .unwrap();
    sdk.edit("rename", None, |edit| edit.rename_mesh(MESH_ID, "renamed"))
        .unwrap();
    let state = sdk.history_state();
    assert_eq!((state.undo_steps, state.redo_steps), (2, 0));
    assert!(state.estimated_bytes > 0);
    assert!(state.estimated_bytes <= state.max_bytes);
    sdk.undo().unwrap();
    sdk.undo().unwrap();
    assert!(sdk.mesh_ids().is_empty());
    assert_eq!(sdk.asset_ids(), &[ASSET_ID]);
    assert_eq!(sdk.undo().unwrap_err().code.as_ref(), "NOTHING_TO_UNDO");
}

#[test]
fn budget_rejection_preserves_redo_branch() {
    let baseline = session(HistoryLimits::default()).estimated_content_bytes();
    let mut trial = session(HistoryLimits::default());
    trial
        .edit("asset", None, |edit| edit.create_asset(asset()))
        .unwrap();
    let asset_bytes = trial.estimated_content_bytes();
    let limits = HistoryLimits {
        max_steps: 50,
        max_bytes: baseline + asset_bytes + 512,
    };
    let mut sdk = session(limits);
    sdk.edit("asset", None, |edit| edit.create_asset(asset()))
        .unwrap();
    sdk.undo().unwrap();
    assert_eq!(sdk.history_lengths(), (0, 1));
    let version = sdk.version();
    let asset = asset();
    let mesh = rectangle_mesh(
        MESH_ID,
        "mesh",
        ASSET_ID,
        Vec2::new(0.0, 0.0),
        Vec2::new(10.0, 10.0),
    )
    .unwrap();
    let result = sdk.edit("too large", None, |edit| {
        edit.create_asset(asset)?;
        edit.create_mesh(mesh)
    });
    assert_eq!(result.unwrap_err().code.as_ref(), "HISTORY_LIMIT_EXCEEDED");
    assert_eq!(sdk.version(), version);
    assert_eq!(sdk.history_lengths(), (0, 1));
    sdk.redo().unwrap();
    assert_eq!(sdk.asset_ids(), &[ASSET_ID]);
}

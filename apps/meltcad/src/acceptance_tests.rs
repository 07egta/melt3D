//! Optional native file-workflow acceptance with inspectable output artifacts.

use super::*;
use crate::export_formats::Format;
use crate::export_job::{Job, Options};

#[test]
#[ignore = "set MELTCAD_QA_OUT to a dedicated output directory"]
fn native_save_reopen_and_mesh_export_acceptance() {
    let dir = PathBuf::from(std::env::var_os("MELTCAD_QA_OUT").expect("MELTCAD_QA_OUT is required"));
    std::fs::create_dir_all(&dir).expect("output directory");
    let ctx = egui::Context::default();
    let mut app = App::new(true, None);
    app.add_shape(&ctx, Kind::Box);
    app.settle(&ctx);
    assert_eq!(app.scene.shape_count(), 2);

    let project = dir.join("windows-smoke.meltcad");
    app.path = Some(project.clone());
    assert!(app.try_save(&ctx, false));
    assert!(!app.dirty);
    // Exercise overwriting an existing project, then reopen through the app loader.
    app.scene.groups[0].shapes[0].pos[0] = 4.0;
    app.dirty = true;
    assert!(app.try_save(&ctx, false));
    let reopened = App::new(true, Some(project));
    assert_eq!(reopened.scene, app.scene);
    assert!(!reopened.dirty);

    let (size, rgba) = crate::cpu_render::sheet(&reopened.scene, 128);
    let png = crate::png::encode_rgba(size as u32, size as u32, &rgba).expect("PNG encoding");
    std::fs::write(dir.join("windows-smoke-four-views.png"), png).expect("four-view picture");

    let mut results = Vec::new();
    for format in [Format::Stl, Format::ThreeMf] {
        let path = dir.join(format!("windows-smoke.{}", format.ext()));
        let options = Options { format, tolerance: 1.0, ..Options::default() };
        let job = Job::start(reopened.scene.clone(), None, options, path.clone());
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(30);
        let summary = loop {
            if let Some(result) = job.poll() {
                break result.expect("native export worker");
            }
            assert!(std::time::Instant::now() < deadline, "export worker timed out");
            std::thread::sleep(std::time::Duration::from_millis(5));
        };
        assert!(summary.closed && summary.tris > 100);
        let imported = crate::import_mesh::import_file(&path, &Default::default()).expect("read exported mesh back");
        assert!(imported.health.is_closed());
        assert_eq!(imported.mesh.triangles.len(), summary.tris);
        let bytes = std::fs::metadata(&path).expect("output file exists").len();
        assert!(bytes > 1000);
        results.push(serde_json::json!({
            "format": format.ext(), "triangles": summary.tris, "bytes": bytes,
            "closed": summary.closed, "open_edges": imported.health.open_edges,
            "nonmanifold_edges": imported.health.nonmanifold_edges,
        }));
    }
    let report = serde_json::json!({"shapes": 2, "save_overwrite_reopen": true, "exports": results});
    std::fs::write(dir.join("native-acceptance.json"), serde_json::to_vec_pretty(&report).expect("report JSON")).expect("report file");
}

//! Write a native document without truncating the previous saved copy.
//!
//! The existing JSON representation is streamed through a bounded I/O buffer to a
//! unique file beside the destination. Only a complete, flushed and synced file
//! replaces the destination; the old path is never moved aside first.

use std::fs::{self, File, OpenOptions, Permissions};
use std::io::{self, BufWriter, Write};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

use crate::model::Scene;

const WRITE_BUFFER_BYTES: usize = 64 * 1024;
static NEXT_FILE: AtomicU64 = AtomicU64::new(0);

/// Keep the project's original path and scene unchanged when serialization,
/// flushing or replacement fails. No full JSON String is retained by this path.
pub(super) fn save(path: &Path, scene: &Scene) -> io::Result<()> {
    std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        write_atomic_with(
            path,
            |writer| {
                // The same guard as Scene::to_json: repeated mesh volumes remain
                // references to the first volume, not extra copies in the file.
                let _guard = crate::mesh::SaveGuard::begin();
                serde_json::to_writer_pretty(writer, scene)
                    .map_err(|error| io::Error::new(error.io_error_kind().unwrap_or(io::ErrorKind::InvalidData), error))
            },
            |temporary, destination| fs::rename(temporary, destination),
        )
    }))
    .unwrap_or_else(|_| Err(io::Error::other("作品データの保存処理が中断されました。以前の保存内容は置き換えていません")))
}

fn unique_name() -> String {
    let stamp = SystemTime::now().duration_since(UNIX_EPOCH).unwrap_or_default().as_nanos();
    let sequence = NEXT_FILE.fetch_add(1, Ordering::Relaxed);
    format!(".meltcad-save-{}-{stamp}-{sequence}.tmp", std::process::id())
}

/// An unfinished staged write is removed even when its serializer unwinds.
struct StagedFile {
    path: PathBuf,
    committed: bool,
}

impl StagedFile {
    fn create(parent: &Path) -> io::Result<(Self, File)> {
        for _ in 0..32 {
            let path = parent.join(unique_name());
            match OpenOptions::new().write(true).create_new(true).open(&path) {
                Ok(file) => return Ok((Self { path, committed: false }, file)),
                Err(error) if error.kind() == io::ErrorKind::AlreadyExists => continue,
                Err(error) => return Err(error),
            }
        }
        Err(io::Error::new(io::ErrorKind::AlreadyExists, "保存用の一時ファイルを作れませんでした"))
    }
}

impl Drop for StagedFile {
    fn drop(&mut self) {
        if !self.committed {
            let _ = fs::remove_file(&self.path);
        }
    }
}

fn destination(path: &Path) -> io::Result<(PathBuf, Option<Permissions>)> {
    // Saving through a symbolic link must update its target, not replace the link
    // itself. Refuse an unresolved link rather than silently changing its identity.
    let target = match fs::symlink_metadata(path) {
        Ok(metadata) if metadata.file_type().is_symlink() => fs::canonicalize(path)?,
        Ok(_) => path.to_path_buf(),
        Err(error) if error.kind() == io::ErrorKind::NotFound => path.to_path_buf(),
        Err(error) => return Err(error),
    };
    let permissions = match fs::metadata(&target) {
        Ok(metadata) => {
            if !metadata.is_file() {
                return Err(io::Error::new(io::ErrorKind::InvalidInput, "保存先には通常のファイルを指定してください"));
            }
            let permissions = metadata.permissions();
            if permissions.readonly() {
                return Err(io::Error::new(io::ErrorKind::PermissionDenied, "保存先が読み取り専用です"));
            }
            Some(permissions)
        }
        Err(error) if error.kind() == io::ErrorKind::NotFound => None,
        Err(error) => return Err(error),
    };
    Ok((target, permissions))
}

fn write_atomic_with<W, R>(path: &Path, write: W, replace: R) -> io::Result<()>
where
    W: FnOnce(&mut dyn Write) -> io::Result<()>,
    R: FnOnce(&Path, &Path) -> io::Result<()>,
{
    let (target, permissions) = destination(path)?;
    if target.file_name().is_none() {
        return Err(io::Error::new(io::ErrorKind::InvalidInput, "保存先のファイル名がありません"));
    }
    let parent = target.parent().filter(|parent| !parent.as_os_str().is_empty()).unwrap_or_else(|| Path::new("."));
    let (mut staged, file) = StagedFile::create(parent)?;
    // The closure owns the handle, so every early return closes it before the
    // staging guard tries cleanup (also required by Windows file sharing rules).
    let result = (|| {
        let mut file = file;
        // Apply the existing protection before writing any document bytes, not
        // only after serialization has exposed a temporary copy on disk.
        if let Some(permissions) = permissions {
            file.set_permissions(permissions)?;
        }
        {
            let mut writer = BufWriter::with_capacity(WRITE_BUFFER_BYTES, &mut file);
            write(&mut writer)?;
            writer.flush()?;
        }
        file.sync_all()?;
        drop(file);
        // A single replacement: the previous document stays at its original
        // path throughout all preparation and is never temporarily absent.
        replace(&staged.path, &target)
    })();
    if result.is_ok() {
        staged.committed = true;
        // Best effort metadata durability. A successful replacement is already
        // committed, so a directory-sync limitation is not reported as no save.
        #[cfg(unix)]
        {
            let _ = File::open(parent).and_then(|directory| directory.sync_all());
        }
    }
    result
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::mesh::SharedVolume;
    use crate::model::{Kind, Material, Shape};
    use crate::sdf_volume::Volume;

    struct ScratchDir(PathBuf);

    impl ScratchDir {
        fn new() -> Self {
            let path = std::env::temp_dir().join(unique_name());
            fs::create_dir(&path).expect("create isolated save test directory");
            Self(path)
        }

        fn file(&self) -> PathBuf {
            self.0.join("作品.meltcad")
        }

        fn entries(&self) -> usize {
            fs::read_dir(&self.0).expect("read save test directory").count()
        }
    }

    impl Drop for ScratchDir {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }

    fn mesh_scene(axis: u32, volumes: usize) -> Scene {
        let mut scene = Scene::default();
        let count = (axis as usize).pow(3);
        for index in 0..volumes {
            let volume = SharedVolume::new(Volume {
                dims: [axis; 3],
                origin: [-1.0; 3],
                cell: 1.0,
                data: (0..count)
                    .map(|node| {
                        // Deterministic, bounded low-compression fixture: this is
                        // valid volume data, not a claim about a typical model.
                        let mixed = (node as u32).wrapping_mul(747_796_405).wrapping_add((index as u32).wrapping_mul(2_891_336_453));
                        let mixed = ((mixed >> ((mixed >> 28) + 4)) ^ mixed).wrapping_mul(277_803_737);
                        (((mixed >> 22) ^ mixed) as i16) as f32 / 64.0
                    })
                    .collect(),
            });
            let shape = Shape::mesh(volume, "共有メッシュ", "fixture.obj", [axis as f32; 3], [0.0; 3], Material::default());
            scene.groups[0].shapes.push(shape.clone());
            scene.groups[0].shapes.push(shape);
        }
        scene.dedupe_uids();
        scene
    }

    #[test]
    fn streamed_document_bytes_match_existing_json_including_shared_meshes() {
        let root = ScratchDir::new();
        let path = root.file();
        let mut shapes = Scene::default();
        for kind in Kind::ALL {
            if kind != Kind::Mesh {
                let mut shape = Shape::new(kind, [12.0, -5.0, 20.0], Material::default());
                shape.name = "日本語・\"引用\"・改行\nの形".into();
                shapes.groups[0].shapes.push(shape);
            }
        }
        for scene in [Scene::default(), shapes, mesh_scene(8, 2)] {
            let expected = scene.to_json();
            assert!(!expected.is_empty());
            save(&path, &scene).expect("stream scene to disk");
            assert_eq!(fs::read(&path).expect("saved bytes"), expected.as_bytes(), "format and shared-volume references must remain identical");
            assert_eq!(root.entries(), 1, "no staging file remains after replacement");
        }
        let text = fs::read_to_string(&path).expect("saved mesh scene");
        let (restored, notes) = Scene::from_json_notes(&text).expect("reopen mesh scene");
        assert!(notes.is_empty(), "{notes:?}");
        assert_eq!(restored.shape_count(), 5);
        let first = restored.shape(0, 1).and_then(|shape| shape.volume.as_ref()).expect("first mesh volume");
        let duplicate = restored.shape(0, 2).and_then(|shape| shape.volume.as_ref()).expect("duplicate mesh volume");
        assert_eq!(first.key(), duplicate.key(), "duplicates must still share their mesh allocation");
    }

    #[test]
    fn a_partial_write_failure_preserves_previous_bytes_and_cleans_staging() {
        let root = ScratchDir::new();
        let path = root.file();
        fs::write(&path, b"previous saved document").expect("old document");
        let error = write_atomic_with(
            &path,
            |writer| {
                writer.write_all(&vec![b'x'; WRITE_BUFFER_BYTES + 17])?;
                Err(io::Error::new(io::ErrorKind::WriteZero, "injected full disk"))
            },
            |temporary, destination| fs::rename(temporary, destination),
        )
        .expect_err("partial write cannot commit");
        assert_eq!(error.kind(), io::ErrorKind::WriteZero);
        assert_eq!(fs::read(&path).expect("old document survives"), b"previous saved document");
        assert_eq!(root.entries(), 1, "failed staging file is cleaned up");
    }

    #[test]
    fn failed_replacement_keeps_original_path_and_complete_staging_is_cleaned() {
        let root = ScratchDir::new();
        let path = root.file();
        fs::write(&path, b"previous saved document").expect("old document");
        let result = write_atomic_with(
            &path,
            |writer| writer.write_all(b"complete new document"),
            |temporary, destination| {
                assert_eq!(temporary.parent(), destination.parent(), "replacement stays on the same filesystem");
                assert_eq!(fs::read(destination).expect("old path is still present"), b"previous saved document");
                assert_eq!(fs::read(temporary).expect("staging is flushed and closed"), b"complete new document");
                Err(io::Error::new(io::ErrorKind::PermissionDenied, "injected replacement failure"))
            },
        );
        assert!(result.is_err());
        assert_eq!(fs::read(&path).expect("old document survives"), b"previous saved document");
        assert_eq!(root.entries(), 1);
    }

    #[test]
    fn interrupted_serialization_cleans_staging_without_touching_previous_save() {
        let root = ScratchDir::new();
        let path = root.file();
        fs::write(&path, b"previous saved document").expect("old document");
        let result = std::panic::catch_unwind(|| {
            write_atomic_with(
                &path,
                |writer| {
                    writer.write_all(b"partial")?;
                    panic!("injected serializer panic");
                },
                |temporary, destination| fs::rename(temporary, destination),
            )
        });
        assert!(result.is_err());
        assert_eq!(fs::read(&path).expect("old document survives"), b"previous saved document");
        assert_eq!(root.entries(), 1);
    }

    #[test]
    fn successful_replacement_preserves_original_until_final_step() {
        let root = ScratchDir::new();
        let path = root.file();
        fs::write(&path, b"old").expect("old document");
        write_atomic_with(
            &path,
            |writer| writer.write_all(b"new"),
            |temporary, destination| {
                assert_eq!(fs::read(destination).expect("old path is still present"), b"old");
                assert_eq!(fs::read(temporary).expect("complete replacement"), b"new");
                fs::rename(temporary, destination)
            },
        )
        .expect("replace complete document");
        assert_eq!(fs::read(&path).expect("new document"), b"new");
        assert_eq!(root.entries(), 1);
    }

    #[test]
    fn read_only_destination_is_not_replaced() {
        let root = ScratchDir::new();
        let path = root.file();
        fs::write(&path, b"read only saved document").expect("old document");
        let original = fs::metadata(&path).expect("original metadata").permissions();
        let mut read_only = original.clone();
        read_only.set_readonly(true);
        fs::set_permissions(&path, read_only).expect("protect document");
        let result = save(&path, &Scene::default());
        let actual = fs::read(&path).expect("read protected document");
        fs::set_permissions(&path, original).expect("restore test file permissions");
        assert_eq!(result.expect_err("read only file cannot be replaced").kind(), io::ErrorKind::PermissionDenied);
        assert_eq!(actual, b"read only saved document");
        assert_eq!(root.entries(), 1);
    }

    #[cfg(unix)]
    #[test]
    fn saving_through_a_symlink_keeps_the_link_and_updates_its_target() {
        let root = ScratchDir::new();
        let target = root.file();
        let link = root.0.join("linked.meltcad");
        fs::write(&target, b"old").expect("old document");
        std::os::unix::fs::symlink(&target, &link).expect("create document link");
        let scene = Scene::default();
        save(&link, &scene).expect("save linked document");
        assert!(fs::symlink_metadata(&link).expect("link metadata").file_type().is_symlink());
        assert_eq!(fs::read(&target).expect("link target"), scene.to_json().as_bytes());
        assert_eq!(root.entries(), 2);
    }

    #[cfg(unix)]
    #[test]
    fn saving_preserves_existing_file_permissions() {
        use std::os::unix::fs::PermissionsExt;
        let root = ScratchDir::new();
        let path = root.file();
        fs::write(&path, b"old").expect("old document");
        fs::set_permissions(&path, Permissions::from_mode(0o640)).expect("set file permissions");
        save(&path, &Scene::default()).expect("save document");
        assert_eq!(fs::metadata(&path).expect("saved metadata").permissions().mode() & 0o777, 0o640);
    }

    // Run each memory fixture in its own process under /usr/bin/time -v. The same
    // valid 4-volume scene is used; repeated shapes share volumes and the output
    // file hash can be compared without retaining it in this process.
    fn memory_fixture(streaming: bool) {
        let root = ScratchDir::new();
        let path = root.file();
        let scene = mesh_scene(128, 4);
        let started = std::time::Instant::now();
        if streaming {
            save(&path, &scene).expect("stream memory fixture");
        } else {
            fs::write(&path, scene.to_json()).expect("buffer memory fixture");
        }
        let size = fs::metadata(&path).expect("fixture file size").len();
        eprintln!("native document save: streaming={streaming} bytes={size} elapsed_ms={:.3}", started.elapsed().as_secs_f64() * 1000.0);
        // Compute the digest incrementally, so the benchmark itself does not
        // recreate the full serialized-document buffer removed by the change.
        let mut file = File::open(&path).expect("open serialized fixture");
        let mut block = [0u8; WRITE_BUFFER_BYTES];
        let mut hash = 0xcbf2_9ce4_8422_2325_u64;
        loop {
            let count = std::io::Read::read(&mut file, &mut block).expect("read fixture block");
            if count == 0 {
                break;
            }
            for byte in &block[..count] {
                hash = (hash ^ u64::from(*byte)).wrapping_mul(0x0000_0100_0000_01b3);
            }
        }
        eprintln!("native document save: bytes={size} fnv1a64={hash:016x}");
        // Optional external cmp/sha256 validation, without loading the document
        // back into memory or including the copy in the reported save duration.
        if let Some(output) = std::env::var_os("MELTCAD_SAVE_BENCH_OUTPUT") {
            fs::copy(&path, output).expect("retain serialized fixture for byte comparison");
        }
    }

    #[test]
    #[ignore = "isolated-process RSS measurement; compare with streaming_save_peak_memory"]
    fn buffered_save_peak_memory() {
        memory_fixture(false);
    }

    #[test]
    #[ignore = "isolated-process RSS measurement; compare with buffered_save_peak_memory"]
    fn streaming_save_peak_memory() {
        memory_fixture(true);
    }
}

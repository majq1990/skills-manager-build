use std::io::{Cursor, Write};
use std::path::Path;

use anyhow::{Context, Result};
use zip::write::SimpleFileOptions;
use zip::ZipWriter;

/// 把目录 `dir` 的**内容**递归打包成内存中的 zip 字节。
///
/// zip 根目录 = `dir` 的子项（不带 `dir` 这一层顶层目录），保留子目录结构。
/// 这正好满足企业服务端「zip 根必须直接含 SKILL.md」的契约：
/// 当 `dir` 直接含 SKILL.md 时，打出的 zip 根也直接含 SKILL.md。
///
/// 仅依赖 std::fs 递归 + `zip` crate，不引入 walkdir 等新依赖。
pub fn pack_dir(dir: &Path) -> Result<Vec<u8>> {
    if !dir.is_dir() {
        anyhow::bail!("Not a directory: {}", dir.display());
    }

    let buf = Cursor::new(Vec::<u8>::new());
    let mut zip = ZipWriter::new(buf);
    let options = SimpleFileOptions::default();

    add_dir_contents(&mut zip, dir, "", options)?;

    let cursor = zip.finish().context("Failed to finalize zip")?;
    Ok(cursor.into_inner())
}

/// 递归把 `dir` 下的条目写进 zip，`prefix` 是当前在 zip 内的相对路径前缀（用 `/` 分隔，无前导斜杠）。
fn add_dir_contents(
    zip: &mut ZipWriter<Cursor<Vec<u8>>>,
    dir: &Path,
    prefix: &str,
    options: SimpleFileOptions,
) -> Result<()> {
    let mut entries: Vec<_> = std::fs::read_dir(dir)
        .with_context(|| format!("Failed to read dir: {}", dir.display()))?
        .collect::<std::io::Result<Vec<_>>>()
        .with_context(|| format!("Failed to enumerate dir: {}", dir.display()))?;
    // 稳定排序，保证打包结果可复现。
    entries.sort_by_key(|e| e.file_name());

    for entry in entries {
        let path = entry.path();
        let file_name = entry.file_name();
        let name = file_name.to_string_lossy();
        let zip_path = if prefix.is_empty() {
            name.to_string()
        } else {
            format!("{}/{}", prefix, name)
        };

        let file_type = entry
            .file_type()
            .with_context(|| format!("Failed to stat: {}", path.display()))?;

        if file_type.is_dir() {
            // 显式写入目录条目（带尾斜杠），再递归内容。
            zip.add_directory(format!("{}/", zip_path), options)
                .with_context(|| format!("Failed to add dir entry: {}", zip_path))?;
            add_dir_contents(zip, &path, &zip_path, options)?;
        } else if file_type.is_file() {
            let bytes = std::fs::read(&path)
                .with_context(|| format!("Failed to read file: {}", path.display()))?;
            zip.start_file(&zip_path, options)
                .with_context(|| format!("Failed to start zip entry: {}", zip_path))?;
            zip.write_all(&bytes)
                .with_context(|| format!("Failed to write zip entry: {}", zip_path))?;
        }
        // 软链接等其它类型跳过（技能目录一般不含）。
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Read;

    #[test]
    fn pack_dir_puts_skill_md_at_zip_root() {
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path();
        std::fs::write(root.join("SKILL.md"), "---\nname: Demo\n---\n").unwrap();
        std::fs::create_dir_all(root.join("scripts")).unwrap();
        std::fs::write(root.join("scripts").join("run.sh"), "echo hi").unwrap();

        let bytes = pack_dir(root).unwrap();
        let reader = Cursor::new(bytes);
        let mut archive = zip::ZipArchive::new(reader).unwrap();

        // SKILL.md 必须在根（无顶层目录）。
        let mut skill_md = archive.by_name("SKILL.md").unwrap();
        let mut content = String::new();
        skill_md.read_to_string(&mut content).unwrap();
        assert!(content.contains("name: Demo"));
        drop(skill_md);

        // 子目录结构保留。
        assert!(archive.by_name("scripts/run.sh").is_ok());
    }
}

use std::fs::File;
use std::io::{BufReader, Read};
use std::path::{Path, PathBuf};
use std::time::Duration;

use futures::StreamExt;
use serde::Serialize;
use sha2::{Digest, Sha256};
use tokio::io::AsyncWriteExt;

use super::resources;

const NODE_DOWNLOAD_TIMEOUT_SECS: u64 = 600;

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct NodeRuntimeStatus {
    pub installed: bool,
    pub node_path: Option<String>,
    pub npm_path: Option<String>,
    pub version: String,
    pub install_dir: String,
    pub source: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct NodeRuntimeAsset {
    pub version: &'static str,
    pub platform: &'static str,
    pub arch: &'static str,
    pub archive_name: &'static str,
    pub sha256: &'static str,
}

impl NodeRuntimeAsset {
    pub fn download_url(&self) -> String {
        format!(
            "https://nodejs.org/dist/{}/{}",
            self.version, self.archive_name
        )
    }
}

const NODE_RUNTIME_ASSETS: &[NodeRuntimeAsset] = &[
    NodeRuntimeAsset {
        version: resources::NODE_RUNTIME_VERSION,
        platform: "darwin",
        arch: "x64",
        archive_name: "node-v24.11.1-darwin-x64.tar.gz",
        sha256: "096081b6d6fcdd3f5ba0f5f1d44a47e83037ad2e78eada26671c252fe64dd111",
    },
    NodeRuntimeAsset {
        version: resources::NODE_RUNTIME_VERSION,
        platform: "darwin",
        arch: "arm64",
        archive_name: "node-v24.11.1-darwin-arm64.tar.gz",
        sha256: "b05aa3a66efe680023f930bd5af3fdbbd542794da5644ca2ad711d68cbd4dc35",
    },
    NodeRuntimeAsset {
        version: resources::NODE_RUNTIME_VERSION,
        platform: "linux",
        arch: "x64",
        archive_name: "node-v24.11.1-linux-x64.tar.gz",
        sha256: "58a5ff5cc8f2200e458bea22e329d5c1994aa1b111d499ca46ec2411d58239ca",
    },
    NodeRuntimeAsset {
        version: resources::NODE_RUNTIME_VERSION,
        platform: "linux",
        arch: "arm64",
        archive_name: "node-v24.11.1-linux-arm64.tar.gz",
        sha256: "0dc93ec5c798b0d347f068db6d205d03dea9a71765e6a53922b682b91265d71f",
    },
    NodeRuntimeAsset {
        version: resources::NODE_RUNTIME_VERSION,
        platform: "win",
        arch: "x64",
        archive_name: "node-v24.11.1-win-x64.zip",
        sha256: "5355ae6d7c49eddcfde7d34ac3486820600a831bf81dc3bdca5c8db6a9bb0e76",
    },
    NodeRuntimeAsset {
        version: resources::NODE_RUNTIME_VERSION,
        platform: "win",
        arch: "arm64",
        archive_name: "node-v24.11.1-win-arm64.zip",
        sha256: "ce9ee4e547ebdff355beb48e309b166c24df6be0291c9eaf103ce15f3de9e5b4",
    },
];

pub fn current_node_runtime_asset() -> Result<NodeRuntimeAsset, String> {
    let spec = resources::private_node_platform_spec();
    NODE_RUNTIME_ASSETS
        .iter()
        .copied()
        .find(|asset| asset.platform == spec.platform && asset.arch == spec.arch)
        .ok_or_else(|| {
            format!(
                "当前平台暂不支持一键安装 Node.js: {}-{}",
                spec.platform, spec.arch
            )
        })
}

#[cfg(test)]
pub fn sha256_hex(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}

fn file_sha256_hex(path: &Path) -> Result<String, String> {
    let file = File::open(path).map_err(|e| format!("打开下载文件失败: {e}"))?;
    let mut reader = BufReader::new(file);
    let mut hasher = Sha256::new();
    let mut buffer = [0_u8; 64 * 1024];
    loop {
        let read = reader
            .read(&mut buffer)
            .map_err(|e| format!("读取下载文件失败: {e}"))?;
        if read == 0 {
            break;
        }
        hasher.update(&buffer[..read]);
    }
    Ok(format!("{:x}", hasher.finalize()))
}

pub fn status() -> Result<NodeRuntimeStatus, String> {
    let root = resources::private_node_runtime_root()?;
    let private_status = status_from_root(&root)?;
    if private_status.installed {
        return Ok(private_status);
    }

    match resources::detect_node().and_then(|node_path| {
        let npm_path = resources::detect_npm(&node_path)?;
        Ok((node_path, npm_path))
    }) {
        Ok((node_path, npm_path)) => Ok(NodeRuntimeStatus {
            installed: true,
            node_path: Some(node_path.to_string_lossy().to_string()),
            npm_path: Some(npm_path.to_string_lossy().to_string()),
            version: resources::NODE_RUNTIME_VERSION.to_string(),
            install_dir: root
                .join(resources::NODE_RUNTIME_VERSION)
                .join(resources::private_node_platform_spec().install_dir)
                .to_string_lossy()
                .to_string(),
            source: "system".to_string(),
        }),
        Err(_) => Ok(private_status),
    }
}

pub fn status_from_root(root: &Path) -> Result<NodeRuntimeStatus, String> {
    let spec = resources::private_node_platform_spec();
    let install_dir = root
        .join(resources::NODE_RUNTIME_VERSION)
        .join(spec.install_dir);
    let node_path = resources::private_node_executable_from_root(root, &spec);
    let npm_path = resources::private_npm_executable_for_node(&node_path);
    let installed = node_path.exists() && npm_path.exists();

    Ok(NodeRuntimeStatus {
        installed,
        node_path: installed.then(|| node_path.to_string_lossy().to_string()),
        npm_path: installed.then(|| npm_path.to_string_lossy().to_string()),
        version: resources::NODE_RUNTIME_VERSION.to_string(),
        install_dir: install_dir.to_string_lossy().to_string(),
        source: if installed { "private" } else { "missing" }.to_string(),
    })
}

pub async fn install<F>(on_log: F) -> Result<NodeRuntimeStatus, String>
where
    F: Fn(String) + Send + Sync,
{
    let root = resources::private_node_runtime_root()?;
    let asset = current_node_runtime_asset()?;
    let spec = resources::private_node_platform_spec();
    let install_dir = root.join(asset.version).join(spec.install_dir);
    let temp_dir = root
        .join("tmp")
        .join(format!("{}-download", spec.install_dir));
    let archive_path = root.join("downloads").join(asset.archive_name);

    on_log(format!(
        "准备安装 Node.js {} ({})",
        asset.version, spec.install_dir
    ));
    std::fs::create_dir_all(
        archive_path
            .parent()
            .ok_or_else(|| "无法解析下载目录".to_string())?,
    )
    .map_err(|e| format!("创建下载目录失败: {e}"))?;
    std::fs::create_dir_all(temp_dir.parent().unwrap_or(&root))
        .map_err(|e| format!("创建临时目录失败: {e}"))?;

    download_archive(asset, &archive_path, &on_log).await?;
    verify_archive(&archive_path, asset.sha256, &on_log)?;

    replace_dir(&temp_dir, &root)?;
    std::fs::create_dir_all(&temp_dir).map_err(|e| format!("创建解压目录失败: {e}"))?;
    on_log("正在解压 Node.js...".to_string());
    extract_archive(&archive_path, &temp_dir, asset)?;

    replace_dir(&install_dir, &root)?;
    std::fs::create_dir_all(
        install_dir
            .parent()
            .ok_or_else(|| "无法解析安装目录".to_string())?,
    )
    .map_err(|e| format!("创建安装父目录失败: {e}"))?;
    let extracted_root = find_extracted_root(&temp_dir, asset)?;
    move_dir_contents(&extracted_root, &install_dir)?;
    std::fs::remove_dir_all(&temp_dir).ok();
    set_runtime_executable_permissions(&install_dir)?;

    let status = status_from_root(&root)?;
    if !status.installed {
        return Err("Node.js 运行时安装完成但未找到 node/npm，请检查安装日志".to_string());
    }
    on_log("Node.js 运行环境安装完成".to_string());
    Ok(status)
}

async fn download_archive<F>(
    asset: NodeRuntimeAsset,
    archive_path: &Path,
    on_log: &F,
) -> Result<(), String>
where
    F: Fn(String) + Send + Sync,
{
    let url = asset.download_url();
    on_log(format!("正在下载 {url}"));
    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(NODE_DOWNLOAD_TIMEOUT_SECS))
        .build()
        .map_err(|e| format!("创建 Node.js 下载客户端失败: {e}"))?;
    let response = client
        .get(&url)
        .header("User-Agent", "CCG-Switch-Node-Runtime")
        .send()
        .await
        .map_err(|e| format!("下载 Node.js 失败: {e}"))?
        .error_for_status()
        .map_err(|e| format!("下载 Node.js 返回失败状态: {e}"))?;
    let total = response.content_length().unwrap_or(0);
    let mut file = tokio::fs::File::create(archive_path)
        .await
        .map_err(|e| format!("创建下载文件失败: {e}"))?;
    let mut stream = response.bytes_stream();
    let mut downloaded = 0_u64;
    let mut last_logged_percent = 0_u64;

    while let Some(chunk) = stream.next().await {
        let chunk = chunk.map_err(|e| format!("下载 Node.js 中断: {e}"))?;
        file.write_all(&chunk)
            .await
            .map_err(|e| format!("写入 Node.js 下载文件失败: {e}"))?;
        downloaded += chunk.len() as u64;
        if total > 0 {
            let percent = (downloaded * 100 / total).min(100);
            if percent == 100 || percent >= last_logged_percent + 10 {
                on_log(format!("下载进度 {percent}%"));
                last_logged_percent = percent;
            }
        }
    }
    file.flush()
        .await
        .map_err(|e| format!("保存 Node.js 下载文件失败: {e}"))?;
    Ok(())
}

fn verify_archive<F>(archive_path: &Path, expected_sha256: &str, on_log: &F) -> Result<(), String>
where
    F: Fn(String) + Send + Sync,
{
    on_log("正在校验 SHA256...".to_string());
    let actual = file_sha256_hex(archive_path)?;
    if actual != expected_sha256 {
        return Err(format!(
            "Node.js 下载文件校验失败: 期望 {expected_sha256}, 实际 {actual}"
        ));
    }
    Ok(())
}

fn replace_dir(target: &Path, allowed_root: &Path) -> Result<(), String> {
    if !target.starts_with(allowed_root) {
        return Err(format!(
            "安全错误：目标目录超出运行时目录范围: {}",
            target.display()
        ));
    }
    if target.exists() {
        std::fs::remove_dir_all(target)
            .map_err(|e| format!("清理目录失败 {}: {e}", target.display()))?;
    }
    Ok(())
}

fn find_extracted_root(temp_dir: &Path, asset: NodeRuntimeAsset) -> Result<PathBuf, String> {
    let expected_name = asset
        .archive_name
        .strip_suffix(".zip")
        .or_else(|| asset.archive_name.strip_suffix(".tar.gz"))
        .unwrap_or(asset.archive_name);
    let expected = temp_dir.join(expected_name);
    if expected.exists() {
        return Ok(expected);
    }

    let mut dirs = std::fs::read_dir(temp_dir)
        .map_err(|e| format!("读取解压目录失败: {e}"))?
        .flatten()
        .map(|entry| entry.path())
        .filter(|path| path.is_dir());
    dirs.next()
        .ok_or_else(|| "解压完成但未找到 Node.js 根目录".to_string())
}

fn move_dir_contents(from: &Path, to: &Path) -> Result<(), String> {
    std::fs::create_dir_all(to).map_err(|e| format!("创建安装目录失败: {e}"))?;
    for entry in std::fs::read_dir(from).map_err(|e| format!("读取解压目录失败: {e}"))? {
        let entry = entry.map_err(|e| format!("读取解压文件失败: {e}"))?;
        let target = to.join(entry.file_name());
        std::fs::rename(entry.path(), &target)
            .map_err(|e| format!("移动 Node.js 文件失败 {}: {e}", target.display()))?;
    }
    Ok(())
}

fn extract_archive(
    archive_path: &Path,
    destination: &Path,
    asset: NodeRuntimeAsset,
) -> Result<(), String> {
    if asset.archive_name.ends_with(".zip") {
        extract_zip(archive_path, destination)
    } else if asset.archive_name.ends_with(".tar.gz") {
        extract_tar_gz(archive_path, destination)
    } else {
        Err(format!("不支持的 Node.js 归档格式: {}", asset.archive_name))
    }
}

fn extract_zip(archive_path: &Path, destination: &Path) -> Result<(), String> {
    let file = File::open(archive_path).map_err(|e| format!("打开 zip 失败: {e}"))?;
    let mut archive = zip::ZipArchive::new(file).map_err(|e| format!("读取 zip 失败: {e}"))?;
    archive
        .extract(destination)
        .map_err(|e| format!("解压 zip 失败: {e}"))
}

fn extract_tar_gz(archive_path: &Path, destination: &Path) -> Result<(), String> {
    let file = File::open(archive_path).map_err(|e| format!("打开 tar.gz 失败: {e}"))?;
    let decoder = flate2::read::GzDecoder::new(file);
    let mut archive = tar::Archive::new(decoder);
    archive
        .unpack(destination)
        .map_err(|e| format!("解压 tar.gz 失败: {e}"))
}

fn set_runtime_executable_permissions(_install_dir: &Path) -> Result<(), String> {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        for path in [
            _install_dir.join("bin").join("node"),
            _install_dir.join("bin").join("npm"),
            _install_dir.join("bin").join("npx"),
        ] {
            if path.exists() {
                let mut permissions = std::fs::metadata(&path)
                    .map_err(|e| format!("读取执行权限失败 {}: {e}", path.display()))?
                    .permissions();
                permissions.set_mode(0o755);
                std::fs::set_permissions(&path, permissions)
                    .map_err(|e| format!("设置执行权限失败 {}: {e}", path.display()))?;
            }
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn current_asset_is_pinned_to_official_node_archive_for_this_platform() {
        let asset = current_node_runtime_asset().expect("current platform should be supported");

        assert_eq!(asset.version, resources::NODE_RUNTIME_VERSION);
        assert!(asset
            .download_url()
            .starts_with("https://nodejs.org/dist/v24.11.1/"));
        assert_eq!(asset.sha256.len(), 64);
        if cfg!(windows) {
            assert!(asset.archive_name.ends_with(".zip"));
        } else {
            assert!(asset.archive_name.ends_with(".tar.gz"));
        }
    }

    #[test]
    fn sha256_hex_matches_known_digest() {
        assert_eq!(
            sha256_hex(b"abc"),
            "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad",
        );
    }

    #[test]
    fn runtime_status_reports_private_install_when_node_and_npm_exist() {
        let temp = unique_test_dir("node-runtime-status");
        let spec = crate::chat::resources::private_node_platform_spec();
        let node_path = crate::chat::resources::private_node_executable_from_root(&temp, &spec);
        let npm_path = crate::chat::resources::private_npm_executable_for_node(&node_path);
        write_file(&node_path, "node");
        write_file(&npm_path, "npm");

        let status = status_from_root(&temp).expect("status should resolve");

        assert!(status.installed);
        assert_eq!(status.source, "private");
        assert_eq!(
            status.node_path.as_deref(),
            Some(node_path.to_string_lossy().as_ref())
        );
        assert_eq!(
            status.npm_path.as_deref(),
            Some(npm_path.to_string_lossy().as_ref())
        );

        std::fs::remove_dir_all(temp).ok();
    }

    #[test]
    fn runtime_status_reports_missing_when_private_files_do_not_exist() {
        let temp = unique_test_dir("node-runtime-missing");

        let status = status_from_root(&temp).expect("missing status should resolve");

        assert!(!status.installed);
        assert_eq!(status.source, "missing");
        assert_eq!(status.node_path, None);
        assert_eq!(status.npm_path, None);

        std::fs::remove_dir_all(temp).ok();
    }

    #[test]
    fn rejects_archive_with_unexpected_sha256() {
        let temp = unique_test_dir("node-runtime-sha");
        let archive = temp.join("node.tar.gz");
        std::fs::write(&archive, b"wrong").expect("write archive");

        let error = verify_archive(&archive, "00", &|_| {}).expect_err("sha should fail");

        assert!(error.contains("校验失败"));
        std::fs::remove_dir_all(temp).ok();
    }

    fn unique_test_dir(name: &str) -> std::path::PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "ccg-switch-{name}-{}",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .expect("system time")
                .as_nanos()
        ));
        std::fs::create_dir_all(&dir).expect("create test dir");
        dir
    }

    fn write_file(path: &std::path::Path, content: &str) {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).expect("create parent dir");
        }
        std::fs::write(path, content).expect("write file");
    }
}

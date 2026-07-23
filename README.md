# 打印助手（PrintAssist）

仅面向 Windows 的批量文件打印控制桌面应用。参考 Print Conductor 的任务流，使用 Rsbuild + React + Ant Design 前端，以及定制 Tauri 2 / PakePlus 思路的原生壳。

## 已确认能力

- 系统打印机枚举、默认打印机、在线/离线状态、彩色/双面能力读取（Win32）
- 内存打印队列：追加不覆盖，重启清空
- 公共默认设置 + 单文件覆盖（颜色、单双面、翻转、份数、页码范围）
- 文件入口：页面选择文件/文件夹、拖放、命令行参数、经典右键菜单与“发送到”
- Office 文档优先通过本机 Office COM 转 PDF 后打印
- 失败隔离与失败项重试
- GitHub Actions CI / Release
- 检查更新：读取 GitHub Release 最新稳定版并打开下载页

## 开发

```bash
# 工具链
# Node 22+ / npm / Rust 1.97.1（见 rust-toolchain.toml）

npm install
npm run dev

# 原生打印机探针
npm run printer:probe

# 桌面开发
npm run tauri:dev

# 测试与构建
npm run lint
npm test
npm run build
cargo test --manifest-path src-tauri/Cargo.toml
npm run tauri:build
```

## 支持格式

| 类型 | 扩展名 | 说明 |
|---|---|---|
| PDF | `.pdf` | 原生按页打印：保留横向/竖向，按比例铺满纸张 |
| 图片 | `.png .jpg .jpeg .jpe .jfif .bmp .dib .tif .tiff .gif .webp .ico .heic .heif .avif .emf .wmf` | 原生 GDI 打印：保持源方向、按比例铺满纸张（HEIC/AVIF 等依赖系统编解码器） |
| 文本 | `.txt .log .md` | printto |
| Word | `.doc .docx` | 优先 Office COM 直接打印（保留节方向）；失败再转 PDF 原生打印 |
| Excel | `.xls .xlsx` | 优先 Excel COM 直接打印（保留工作表方向）；自定义页码走 PDF |
| PowerPoint | `.ppt .pptx` | 优先 PowerPoint COM 直接打印（保留幻灯片方向）；自定义页码走 PDF |

## 重要限制（首版）

1. **自定义页码范围**：PDF/Office 会先抽取选定页生成临时 PDF 再打印；图片/文本不支持自定义页码。
2. **颜色/双面**会在执行前按打印机能力校验；不支持则阻止，不静默降级。
3. **Windows 11 现代一级右键菜单**未实现；安装后提供经典“显示更多选项”菜单与“发送到”。
4. **更新**：当前版本读取 GitHub Release 并打开下载页；签名静默安装可在配置 CI 签名密钥后升级。
5. **Office** 依赖桌面版 Word/Excel/PowerPoint；仅有关联程序时能力不完整，会提示用户确认。自定义页码必须能成功转 PDF。

## 设计

生产 UI 采用原型方向 A“明亮精密工作台”，见 `docs/design-system.md` 与 `prototypes/`。

## 许可证

私有项目，按仓库所有者约定使用。

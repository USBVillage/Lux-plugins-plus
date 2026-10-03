# Lux-plugins-plus · org.lux.tmdb-plus

[TMDb 元数据增强] —— 一个 [Lux](https://github.com/Qoo-330ml/Lux) 媒体服务器的第三方刮削插件，
目标是把 Emby 神医助手（[StrmAssistant](https://github.com/sjtuross/StrmAssistant)）的能力复刻到 Lux 上。

本仓库基于官方插件仓库 [Qoo-330ml/Lux-plugins](https://github.com/Qoo-330ml/Lux-plugins)
的 `org.lux.tmdb` 插件修改增强，**版权归原作者**；未另行声明许可，仅限个人学习与非商业用途。

## 插件商店安装

在 Lux 管理后台把插件商店目录地址设置为：

```
https://raw.githubusercontent.com/USBVillage/Lux-plugins-plus/main/index.json
```

然后在插件商店中安装 **TMDb 元数据增强**，并将媒体库的刮削器切换为它。

## 相对官方 TMDb 插件的增强

### 1. 剧集组刮削（对应 StrmAssistant「TMDB 剧集组」）

- 开启 `episodeGroupsEnabled` 后，识别/刷新剧集时会按 `episodeGroupStrategy`
  （绝对顺序 / DVD 顺序 / 播出顺序）自动挑选 TMDb 剧集组；
- 剧集元数据返回的 provider id 会带上 `#eg:<组id>` 标记，后续所有季/集请求都会
  **把本地季/集号按剧集组顺序翻译为真实 TMDb 剧集**：本地 `S01E750` 会拿到绝对顺序
  第 750 集的真实标题、简介、首播日期、时长与剧照；
- 本地媒体库的季/集编号保持不变，只有元数据被翻译，不会打乱扫描结构；
- 已识别的剧集会固定使用当时的剧集组（标记随 provider id 持久化），不受后续改配置影响。

### 2. 原语言海报优先（对应 StrmAssistant「获取原语言海报」）

- 请求图片时把条目的 `originalLanguage` 并入 `include_image_language`，
  并把原语言海报排在候选首位；响应带 `originalLanguageMode: true`。
- 可用 `originalLanguagePosterEnabled` 关闭（默认开启）。

### 3. 中文别名搜索增强（对应 StrmAssistant「中文搜索增强」的识别场景）

- 直接检索一无所获时，自动退回宽泛检索，并用 TMDb 别名（译名/别名/原语言名）
  做归一化匹配（忽略大小写、空格、标点），命中者以中文别名作为标题返回。

### 4. 官方 TMDb 插件的全部既有能力

语言回退、替代 API 地址、标题别名替换、人员/合集/预告片/外部 ID 等全部保留。

## 与 StrmAssistant 的功能对照

| StrmAssistant 功能 | Lux 上的实现 |
| --- | --- |
| 提高 strm 起播速度 / 独占模式提取媒体信息 / 媒体信息持久化 / 缩略图增强 | Lux 核心 STRM 探测 + 官方 `org.lux.strm-media-info` 插件 |
| 片头片尾探测增强 | 官方 `org.lux.intro-outro-detector` / `org.lux.theintrodb-chapter-source` 插件 |
| 独立的外挂字幕扫描 | Lux 核心原生支持 |
| 自定义刮削备选语言 / 替代 TMDB 配置 / 演职人员增强 | 本插件与官方 `org.lux.tmdb` 一致 |
| 支持代理服务器 | Lux 核心 `LUX_PROXY_URL` |
| TMDB 剧集组刮削 | **本插件** 剧集组 |
| 获取原语言海报 | **本插件** 原语言海报 |
| 中文搜索增强 | **本插件** 别名兜底搜索（识别/刮削场景）；媒体库内搜索属 Lux 核心 |
| 拼音首字母排序 | **无法通过插件实现**：排序字段由 Lux 宿主在扫描时自行计算，插件 SDK 无注入点 |
| 自动合并同目录视频为多版本 | **无法通过插件实现**：需要扫描器与合并 RPC 支持，当前 SDK v1 无对应能力 |

## 构建

```bash
cargo build --release --bin lux-plugin-tmdb-plus
# aarch64 交叉编译
CARGO_TARGET_AARCH64_UNKNOWN_LINUX_GNU_LINKER=aarch64-linux-gnu-gcc \
  cargo build --release --target aarch64-unknown-linux-gnu --bin lux-plugin-tmdb-plus
python3 scripts/package_plugin.py --id org.lux.tmdb-plus --version <ver> \
  --manifest manifests/org.lux.tmdb-plus.json --binary target/release/lux-plugin-tmdb-plus \
  --platform linux --arch x86_64 --output <out>.zip
```

## 测试

`cargo test --lib tmdb_groups && cargo test --bin lux-plugin-tmdb-plus`

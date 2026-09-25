# Observe 首轮代码 review

日期：2026-09-26。范围：`2fbbff0^..0689699`（包含首个实施提交）。
对照 `OBSERVE-VISUAL-IMPLEMENTATION-PLAN.md` 与已发布的 O1 契约检查；
本次修复保留在工作树中。

## 已修复

- **P1 — 无预期哈希的纹理丢失内容身份。** 资产允许没有预期 SHA-256，
  但场景摘要和 bundle 原先仅保存该空字段，不包含实际 PNG 哈希。
  同路径纹理变化无法可靠区分，生成的包也因哈希不匹配无法重开。
  bundle 现在记录已解析纹理的实际哈希，源 authoring 描述保持原样。
- **P2 — 奇数尺寸 mipmap 丢边。** 固定 2×2 kernel 在 3→1、5→2 等
  缩小中漏掉最后一列/行。改为覆盖完整源区域的面积加权 box filter；
  渲染摘要纳入 `area_box_v2`，避免把新旧生成算法视为同一渲染策略。
- **P2 — 遮罩缓存忽略采样寻址方式。** 同一纹理 view/revision 切换
  Repeat/Clamp 时继续使用旧 mask。缓存签名现在包含寻址策略。
- **P2 — ROI 逆变换溢出。** 有限 ROI 输入和正 scale 不保证可见范围
  或逆 scale 有限；现在在 GPU 工作前返回 `INVALID_VIEW`。
- **P2 — 纹理预算乘法溢出。** 极大宽高的 RGBA 大小乘法可能 panic
  或在 release 中回绕；改为全程 checked arithmetic，并验证 reader
  返回 `OBSERVATION_BUDGET_EXCEEDED`。
- **P2 — 离线 packet 校验不完整。** `1e999` 可绕过 JSON 的
  `parse_constant`；零/负 scale、错误长度和空 ROI 可被读入。
  现在显式拒绝这些输入，并在读取下一文件之前检查剩余累计字节预算。

## 验证

全部通过：

- `cargo test -p kasane-sdk-observe -p kasane-render-wgpu -p kasane-render -p kasane-animation --locked`
  （含实际 GPU mask/blend/offscreen 回归）。
- `cargo test -p kasane-sdk-observe --no-default-features --locked`
  （含追加的超大纹理描述负控制）。
- `cargo clippy -p kasane-sdk-observe -p kasane-render-wgpu -p kasane-python --features kasane-python/observe --all-targets --locked -- -D warnings`
- `cargo fmt --all --check`、`git diff --check`。
- CPython 3.14 observe wheel 构建后安装至独立 `/tmp` venv，从仓库外
  执行 `test_observe.py`（8 tests）与 `test_inspection.py`（3 tests）。

本次未执行整个 workspace 测试、完整 wheel validator 或 Mao 图像集成
probe；已执行的 GPU 测试使用合成 fixture。O2–O7 的展示、对象标记、
对比、诊断和查询等后续功能不属于首轮完成项，此 review 不将其标为完成。

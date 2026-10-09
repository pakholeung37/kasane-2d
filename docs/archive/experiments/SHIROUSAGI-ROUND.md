# Shirousagi：修复首关 S6

日期：2026-09-24。素材基线是 Git `cd04db1` 的 Shirousagi：PSD **24 个** raster
图层，与交付模型的 24 个 ArtMesh 名称对应。冻结 wheel、素材 SHA 和任务设计见
[计划](SHIROUSAGI-PLAN.md)。本轮三位 Luna 各拿到一个不同的故障工程、PSD、三张
正确参考帧和公开 SDK 文档。没有发放 clean rig、目标 ID、错误字段或修正量。

## 判分与结果

作品分为 100 分：公开和未公开姿态逐像素符合原模型 40 分；非目标 authoring 状态、
结构和资源保持 20 分；工程与 model3/MOC3/纹理 package 搬迁后正确 20 分；
从空输出目录重放并重复上述验收 20 分。任一项由冻结验收器给出证据；不按受试者
自报的公开帧结果评分。

| 试次 | 故障 | 作品分 | 原作品 | 空目录重放 | 报告 |
| --- | --- | ---: | --- | --- | --- |
| s6-a | `Warp10` 的一个组合 keyform，X 偏移 | 100/100 | 通过 | 通过 | [report.json](/private/tmp/kasane-shirousagi-s6-final-20260924/host/trials/s6-a/assessments/20260924T102309-14c4ab85/report.json) |
| s6-b | `Warp11` 的另一组合 keyform，X 偏移 | 100/100 | 通过 | 通过 | [report.json](/private/tmp/kasane-shirousagi-s6-final-20260924/host/trials/s6-b/assessments/20260924T102352-e6b33b98/report.json) |
| s6-c | `Warp10` 的第三组合 keyform，Y 偏移 | 100/100 | 通过 | 通过 | [report.json](/private/tmp/kasane-shirousagi-s6-final-20260924/host/trials/s6-c/assessments/20260924T102052-031a4668/report.json) |

每份报告在原作品、搬迁后的工程和搬迁后的 package 上都检查了 11 个姿态
（3 个公开、8 个未公开），并检查结构、资源、对象身份及非目标状态。三种变体的
正控制通过；不修改、修正量错误、改错 keyform 的负控制均失败。三位的解法不同：
A 从同角度开眼形态识别整体平移，B 用相邻角度的闭/开眼差重建，C 在同角度形态
上消除统一偏移。三份作品均通过同一冻结验收器。

此轮使用桌面 subagent 手动发放，未使用受控 runner 记录完整命令轨迹及时间。
三者共享同一系统用户和文件系统，故 **只给作品分**；`independent_success`、
零提示成功率、耗时和 token 指标都不填。任务卡禁止读取仓库和主持人目录，
但共享权限无法证明严格隔离。

## 暴露的问题及处理

- `Edit.replace_scene_binding` 只接受五个拆开的字段，和可直接接受 snapshot 的
  其他 `replace_*` 方法不一致。A 第一次尝试把 `SceneBindingSnapshot` 直接传入而
  遇到 `TypeError`，随后查签名并恢复。SDK 现增加 snapshot 入口，同时保留五字段
  调用；API 文档列出两种形式，CPU 测试覆盖保存重开。
- A、B、C 在探查 mesh 时都曾误读快照字段；文档现明确 `MeshRecordSnapshot`
  使用 `part_id`、`deformer_id`。B 首次重复保存到已有目录被拒，符合已记录的
  项目冲突语义，随后从输入重新运行；没有把这次重试计成 SDK 错误。
- S6 仍主要考察**单个** scene keyform 的异常定位与修复。PSD 在本关用于确认
  24 层身份和静态画面，没有成为需要加工的输入。三位全部通过，说明这种故障
  已不足以继续暴露系统深层问题。

## 下一关的发放门槛

S7 从 24 层 PSD 的静态工程出发，让受试者创作局部动态行为，并保留隐藏层、
对象身份、可编辑关键形态和可搬迁 package。主持人初步试过三个嘴部图层的简单
矩形形变，工程诊断、保存和导出均成功，但画面明显错误。首关因此转为双眼独立
眨眼；公开 API oracle 与正负控制已通过，新的三个 learning 任务包已发放。
具体门槛和发现的导出身份缺口见[计划](SHIROUSAGI-PLAN.md)。原模型和 PSD 工程
默认画面存在细小像素差，本关只对眼部局部效果设相对误差门槛。

## S7：PSD 双眼独立眨眼

同三位 Luna 在 S6 会话中继续，各拿到从同一 24 层 PSD 导入的静态工程、四张交付
模型眼部参考帧和公开 SDK 文档。任务要求创作两个独立 `[0,1]` 参数，保留 24 层、
全部素材和隐藏状态，只在现有眼部图层增加可编辑绑定。判分覆盖四种公开组合、
三个未公开中间组合的眼部裁剪区改善、默认和眼外像素保持、工程及 package 搬迁，
并从空目录重放脚本。正控制和增加合法中间关键帧的控制通过；不修改、只做一眼、
错误 runtime ID、无效形变均被拒绝。

| 试次 | 创作方式 | 作品分 | 原作品 / 空目录重放 | 正式更正版报告 |
| --- | --- | ---: | --- | --- |
| s7-a | 两端关键形态，四顶点眼部网格压扁 | 100/100 | 通过 / 通过 | [report.json](/private/tmp/kasane-shirousagi-s7-corrected-20260924/host/trials/s7-a/assessments/20260924T111849-30faf935/report.json) |
| s7-b | 两端加 0.5 中间关键形态 | 100/100 | 通过 / 通过 | [report.json](/private/tmp/kasane-shirousagi-s7-corrected-20260924/host/trials/s7-b/assessments/20260924T111856-b7c809eb/report.json) |
| s7-c | 两端关键形态，逐眼调整宽度和位置 | 100/100 | 通过 / 通过 | [report.json](/private/tmp/kasane-shirousagi-s7-corrected-20260924/host/trials/s7-c/assessments/20260924T111905-05aa271b/report.json) |

三份原脚本与原任务包中的脚本 SHA-256 相同，新旧任务包的 PSD 与输入工程哈希
相同；更正版仅改评分器的合法 keyform 结构规则，沿用原先全部视觉与交付门槛。
首次冻结评估中，A、C 直接通过；B 的画面、源状态、package 和重放检查也全部
通过，但评分器错误要求绑定恰好只有 `0`、`1` 两个 keyform，因此把符合任务要求的
`0.5` 关键形态判失败。保留了[原失败报告](/private/tmp/kasane-shirousagi-s7-final-20260924/host/trials/s7-b/assessments/20260924T110721-d04301af/report.json)及[更正评估](/private/tmp/kasane-shirousagi-s7-final-20260924/host/trials/s7-b/assessments/corrected-adjudication-20260924.json)，再在新冻结实验里原样重跑 A、B、C；三者正式通过。该问题归因于**实验评分器过窄**，不是 B 的 SDK 使用失败。

S7 准备阶段另暴露两个真实系统缺口，均在发放前修复并重新构建冻结 wheel：

- 新建参数原本无法指定 MOC3 的 runtime ID，导致保存的工程可按参数名称驱动，
  导出的 package 重导入后却失去 `ParamEyeLOpen` / `ParamEyeROpen` 身份。现在
  `create_parameter` 和 `replace_parameter` 支持 `runtime_id`，参数快照也返回它；
  CPU 测试覆盖 package 重导入。
- PSD 导入原本丢弃唯一且合法的图层名，用生成的 UUID 作为 ArtMesh runtime ID，
  package 重导入后 `ArtMesh14` / `ArtMesh15` 等图层身份丢失。现在保留合法唯一
  图层名，重名或不合法名称仍使用稳定回退 ID；PSD 导入测试和真实 package 控制
  覆盖这一行为。

三位都从眼部图层和参考画面定位目标，均产出可编辑工程、可搬迁 package 和可重放
脚本。静态 PSD 的四顶点眼部网格只能近似交付模型的闭眼轮廓；本关预先固定的是
眼部误差相对静态基线至少减半，并未宣称还原原 rig 或在外部实时驱动器测试。
同 S6，此处为共享系统用户下的桌面 subagent 实验，故仅给作品分，不填
`independent_success`、零提示成功率、实际受控运行耗时或 token 指标。

## S8：交付模型的单图层美术修订

S7 稳定后，主持人从真实 PSD 提取目标图层，按公开 RGB 倍率生成修订 PNG，再把
同一颜色规则应用到原交付模型的共享纹理图集。三个变体分别为 `ArtMesh15` 左眼、
`ArtMesh14` 右眼和 `ArtMesh22` 腮红，均保留交付模型的 24 mesh、34 参数及原有
绑定。三者的 UV 区域互不触及其他 mesh。任务包提供原可编辑 rig、PSD、目标
图层修改前后 PNG、修订规则和三张公开动态参考帧；未公开帧覆盖眨眼、转头、
嘴型及呼吸。

主持人已冻结一个使用 `pillow==12.3.0` 的 uv 环境和 S8 验收器。三个变体中，
公开 API 正控制和包含 UV 边缘的另一合法实现均通过；不修改、改错图层、颜色
错误、全图集染色、顺带改 rig 的负控制均失败。三份主持人脚本在原作品、空目录
重放、工程搬迁及 model3/MOC3/纹理 package 搬迁中通过 10 姿态验收。控制结果
仅说明任务和判分可行，不计入受试者成绩。

| 试次 | 目标图层 | 作品分 | 原作品 / 空目录重放 | 报告 |
| --- | --- | ---: | --- | --- |
| s8-a | `ArtMesh15` 左眼 | 100/100 | 通过 / 通过 | [report.json](/private/tmp/kasane-shirousagi-s8-final-20260924/host/trials/s8-a/assessments/20260924T114853-96c8fc6c/report.json) |
| s8-b | `ArtMesh14` 右眼 | 100/100 | 通过 / 通过 | [report.json](/private/tmp/kasane-shirousagi-s8-final-20260924/host/trials/s8-b/assessments/20260924T114512-4a496ed5/report.json) |
| s8-c | `ArtMesh22` 腮红 | 100/100 | 通过 / 通过 | [report.json](/private/tmp/kasane-shirousagi-s8-final-20260924/host/trials/s8-c/assessments/20260924T115550-e3c01216/report.json) |

三份最终报告的 10 姿态目标裁剪区、目标外画面、图集其他区域及 alpha、
原 rig 的非目标状态、工程和 package 搬迁、空目录重放均通过。A 用原 PSD 图层的
缩放图与图集区域核对定位，B 从 UV 和图层轮廓确定边界，C 对腮红做 UV 邻域
搜索。C 发现 PSD 层最近邻缩放与原图集有 378 个 alpha 边缘 texel、4 个可见
RGB texel 差异；它保留图集原 alpha，正式画面仍通过。由此不能把 PSD 缩放图
当成图集的逐像素标准答案，本关以原模型的动态姿态和限定颜色修订作验收。

本关没有发现保存、导出或渲染的核心失败，但暴露两个公开 API 的使用缺口。
交付模型和 PSD 工程的 UV 垂直原点不同，原 Python `Session` 没有可读的方向
属性，受试者只能通过图层样张与渲染反推；现增加只读 `uv_v_origin`，文档解释
其到 PNG 行的映射。B 还因过早删除替换 PNG 的暂存文件而在保存时报错，之后
保持文件到保存和导出结束即恢复；API 文档现明确这段资源生命周期。两项改进
均发生在**正式 S8 wheel 冻结之后**，不回写三位受试者的实验环境或成绩。

三关均为同一组三位 Luna：S6 为各自首轮，S7/S8 为继续学习轮。由于桌面
subagent 共享系统用户和文件系统，仍只报告作品分；不能把 9/9 作品通过解释为
严格隔离条件下的 `independent_success`、零提示成功率、受控耗时或 token 结果。

# JSON 与 CBOR 二进制工程原型对照（2026-09-25）

## 原型范围

在 `kasane-project` 的 `binary-prototype` 可选 feature 下添加 CBOR 编解码入口。JSON 与 CBOR 共享同一个 `ProjectWire`、Document 构建和验证逻辑，并经同一个 `DocumentStore` 打开、校验贴图及保存。CBOR 文件使用 16 字节魔数及长度头和 `.cbor` 清单；没有压缩，也没有改变贴图 PNG。默认工程格式仍为 JSON。

使用 `~/Downloads/live2d-models/Gothic-style Girl/Gothic-style Girl/Gothic-style Girl.model3.json` 转换得到的现有大型工程，含 418 个网格、156 个普通绑定和 5 张 PNG。JSON 清单为 43,034,136 字节，CBOR 清单为 21,537,264 字节；两者的贴图目录逐文件相同，均为 16,849,068 字节。JSON 转 CBOR、CBOR 保存后重开、与 JSON 原文档 `same_content` 均通过；重复保存得到的 CBOR 清单 SHA-256 相同。

## 结果

Apple M4、16 GB 内存、macOS 27、本机 APFS，Release 构建；热文件系统缓存。每个格式独立进程运行 7 次，两轮交换 JSON/CBOR 运行顺序。表中时间是两轮各自的中位数范围，完整打开排除第一次缓存预热，重复保存排除第一次创建目标。内存是 `/usr/bin/time -l` 测得的整次压测进程最大常驻内存，两轮分别测量。

| 指标 | 紧凑 JSON | CBOR 原型 |
| --- | ---: | ---: |
| 清单大小 | 43.03 MB | 21.54 MB |
| 工程含 PNG 总大小 | 59.88 MB | 38.39 MB |
| `decode`，含 Document 构建 | 178–182 ms | 93–94 ms |
| `encode` | 74–76 ms | 28–29 ms |
| 完整打开 | 292–300 ms | 169–177 ms |
| 重复保存 | 430–457 ms | 247–278 ms |
| 压测进程最大常驻内存 | 489–522 MB | 394–425 MB |

完整打开包含读取清单、计算哈希、解码、贴图校验和结构验证；保存包含贴图读取及哈希核对、素材复用检查、编码、临时文件写入、冲突检查及落盘同步。此对照使用 `DocumentStore`，不包含 `DocumentSession` 的历史管理、应用启动或 Godot UI。因压测进程内容不同，上表内存不能与此前 `project_io_stress` 的 725 MB 直接比较；JSON 与 CBOR 两列之间可比较。

## 判断与限制

这个二进制原型对该真实模型确实更快：完整打开约快 40% 以上，重复保存约快 35–45%，清单约减半。先前“没有证据需要迁移”只基于优化后的 JSON 与旧 JSON 的对照；本次实测提供了格式原型的直接证据。

此结果比较的是**当前 JSON 实现**与**这个 CBOR 实现**，不是所有文本和二进制编码的理论极限。JSON 读取路径会先扫描语法与重复键，再反序列化；CBOR 原型目前没有与生产格式同等完备的损坏恢复、兼容性和迁移设计。原型没有接入应用的默认打开/保存界面，也没有测试冷缓存、网络盘或更复杂的模型。是否正式切换仍需确定格式版本策略、旧工程兼容方式、诊断和错误恢复，再用更多真实工程复测。

可读取的样例位于 `output/project-io-stress/gothic-style-girl-cbor-prototype/`，需启用 `binary-prototype` feature 使用原型入口；默认 JSON 打开入口不会识别它。原始逐次数据与内存统计在 `target/project-io-stress/proto-{json,cbor}-bench*.json` 和对应 `*-time*.txt`。默认 `kasane-core`、`kasane-project`、`kasane-sdk` Release 测试通过；启用原型的 `kasane-project` 测试 41 项通过，包含复杂绑定及精确浮点数往返。

```sh
cargo build --release -p kasane-project --features binary-prototype --example project_binary_prototype
target/release/examples/project_binary_prototype prepare "$PWD/output/project-io-stress/gothic-style-girl-optimized" "$PWD/target/project-io-stress/repro-cbor"
/usr/bin/time -l target/release/examples/project_binary_prototype bench json "$PWD/output/project-io-stress/gothic-style-girl-optimized" "$PWD/target/project-io-stress/repro-json-save" 7
/usr/bin/time -l target/release/examples/project_binary_prototype bench cbor "$PWD/target/project-io-stress/repro-cbor" "$PWD/target/project-io-stress/repro-cbor-save" 7
```

CBOR 编解码使用 [`ciborium` 0.2.2](https://docs.rs/ciborium/0.2.2/ciborium/)。

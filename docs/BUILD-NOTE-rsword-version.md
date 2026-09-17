# 构建备忘：本仓库依赖的 rsword 版本，两个可见版本都不全

本仓库 `main` 的代码同时要求 rsword 提供**两样**东西：

| 用到的地方 | 需要 |
| --- | --- |
| `crates/core/src/anchor.rs`（`6cec4d4`，仓库主） | `rsword::model::drawing::drawing_display` |
| `crates/core/bin/layout-trace.rs`、`examples/probe_oracle.rs` | `rsword::bind::native::SessionTable` |

而我这边能拿到的两个 rsWordParser 版本**各缺一半**：

| rsword 版本 | `model::drawing` | `bind::native` |
| --- | --- | --- |
| `origin/main`（`a8d24ea`） | **无** | 有 |
| 本地 `main`（`e5bed96`） | 有 | **无** |

所以：

- **库与集成测试**：用**本地 `main`** 建 worktree 可以跑，13 套全绿。
- **`--features fontenv` 的 4 套测试**与 **`layout-trace` 二进制**：
  两个版本都编译不了。`layout-trace` 是出引擎轨迹的工具，
  所以「引擎 vs Word 逐字形比较」这条链**目前跑不动**。

## 这对测量工作的影响

**不影响判据。** 量具是 Python 的，采集与判定都不经过引擎。
已经判掉的十一批、以及清单上剩下的项，都只用 Word 的读数。

**影响的是「拿量出来的规则去校引擎」那一半**：
`wm compare`（引擎轨迹 vs 采集包）要 `layout-trace`，现在出不了轨迹。
引擎侧的改动因此只能靠单元测试兜住，而不能再用真采集回归。

## 怎么办

要么仓库主把 rsword 的依赖钉到一个同时有这两样的版本，
要么 `anchor.rs` 与 `layout-trace` 二选一地迁到同一版本的 API 上。
**这不是我能替他定的**，所以只记在这里。

建 worktree 的写法（不动兄弟仓库的当前分支）：

```sh
git -C ../rsWordParser worktree add --detach /tmp/rsword-main main
# 把 Cargo.toml 里 rsword 的 path 临时指过去，跑完再改回来
```

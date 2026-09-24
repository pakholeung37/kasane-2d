#!/usr/bin/env python3
"""Local, provider-neutral SDK experiment CLI. See docs/SDK-EXPERIMENTS.md."""
from __future__ import annotations

import argparse
from datetime import datetime, timezone
import hashlib
import json
import os
from pathlib import Path
import re
import shutil
import signal
import subprocess
import sys
import time
from uuid import uuid4

ROOT = Path(__file__).resolve().parents[1]
SUPPORT = ROOT / "tools/sdk_experiment"


def read(path):
    return json.loads(path.read_text(encoding="utf-8"))


def write(path, value):
    path.parent.mkdir(parents=True, exist_ok=True)
    # Evidence is append-only: callers use new IDs, never replace an old report.
    with path.open("x", encoding="utf-8") as stream:
        json.dump(value, stream, ensure_ascii=False, indent=2, allow_nan=False)
        stream.write("\n")


def digest(path):
    return hashlib.sha256(path.read_bytes()).hexdigest()


def hashes(directory, *, exclude=()):
    result = {}
    for parent, directories, files in os.walk(directory):
        directories[:] = sorted(d for d in directories if d != "__pycache__"
                                 and not (Path(parent) == directory and d in exclude))
        for name in directories + sorted(files):
            path = Path(parent) / name
            if path.is_symlink():
                raise ValueError(f"symlinks are not allowed in evidence trees: {path}")
            if path.is_file():
                result[str(path.relative_to(directory))] = digest(path)
    return result


def packet_hashes(packet):
    return hashes(packet)


def env_python(directory):
    return directory / ("Scripts/python.exe" if os.name == "nt" else "bin/python")


def environment_state(lock, packet):
    """Record the shared environment at each run/review; no per-subject envs."""
    python = Path(lock["python"])
    result = subprocess.run([lock["uv"], "pip", "freeze", "--python", str(python)],
                            cwd=packet, env=clean_env(), text=True, capture_output=True, check=True)
    requirements = result.stdout
    location = subprocess.run([str(python), "-I", "-c",
        "import sysconfig; print(sysconfig.get_path('purelib'))"],
        cwd=packet, env=clean_env(), text=True, capture_output=True, check=True)
    sdk_hashes = hashes(Path(location.stdout.strip()) / "kasane")
    return {"requirements": sorted(requirements.splitlines()),
            "sdk_hashes": sdk_hashes, "sdk_unchanged": sdk_hashes == lock["package_hashes"]}


def stamp():
    return datetime.now(timezone.utc).isoformat()


def clean_env():
    env = os.environ.copy()
    for key in ("PYTHONPATH", "PYTHONHOME", "VIRTUAL_ENV", "UV_PROJECT_ENVIRONMENT", "UV_PROJECT"):
        env.pop(key, None)
    env["PYTHONNOUSERSITE"] = "1"
    env["PYTHONDONTWRITEBYTECODE"] = "1"
    return env


def command(argv, cwd, log, timeout, extra_env=None):
    """Capture real wall time/output, including launch failure and timeout evidence."""
    if timeout <= 0:
        raise ValueError("timeout must be positive")
    log.mkdir(parents=True, exist_ok=False)
    started = time.monotonic()
    result = {"argv": argv, "cwd": str(cwd), "started_at": stamp(), "timeout_seconds": timeout}
    env = clean_env()
    env.update(extra_env or {})
    with (log / "stdout.log").open("wb") as out, (log / "stderr.log").open("wb") as err:
        try:
            process = subprocess.Popen(argv, cwd=cwd, env=env, stdout=out, stderr=err,
                                       start_new_session=True)
            try:
                result["returncode"] = process.wait(timeout=timeout)
                result["status"] = "completed" if process.returncode == 0 else "failed"
            except subprocess.TimeoutExpired:
                os.killpg(process.pid, signal.SIGKILL)
                process.wait()
                result.update(status="timeout", returncode=process.returncode)
        except OSError as exc:
            result.update(status="launch_error", returncode=None, error=str(exc))
    result.update(finished_at=stamp(), elapsed_seconds=time.monotonic() - started)
    write(log / "command.json", result)
    return result


def checked(argv, cwd, log, timeout=180):
    result = command(argv, cwd, log, timeout)
    if result["status"] != "completed":
        raise RuntimeError(f"{result['status']}; inspect {log}")
    return (log / "stdout.log").read_text()


def identity(python, cwd, log):
    code = ("import json,sys,platform,kasane; from importlib.metadata import version; "
            "print(json.dumps(dict(python=sys.version,platform=platform.platform(),"
            "package=kasane.__file__,sdk_version=version('kasane'),capabilities=kasane.capabilities())))")
    return json.loads(checked([str(python), "-I", "-c", code], cwd, log))


def initialize(args):
    root = args.experiment.resolve()
    if root.is_relative_to(ROOT):
        raise ValueError("experiment directory must be outside the repository")
    root.mkdir(parents=True, exist_ok=False)
    host = root / "host"
    frozen = host / "frozen"
    frozen.mkdir(parents=True)
    (root / "packets").mkdir()
    wheel = args.wheel.resolve(strict=True)
    if wheel.suffix != ".whl":
        raise ValueError("--wheel must be a wheel file")
    shutil.copy2(wheel, frozen / wheel.name)
    for name in ("worker.py", "tasks.json"):
        shutil.copy2(SUPPORT / name, frozen / name)
    shutil.copy2(Path(__file__), frozen / "sdk_experiments.py")
    for name in ("README.md", "API.md"):
        shutil.copy2(ROOT / "modules/kasane-python" / name, frozen / name)
    for name in ("delivery-transfer.md", "resource-recovery.md", "visual-locate.md", "visual-parent.md", "compose-expression.md", "handoff-revision.md", "shirousagi-repair.md", "shirousagi-blink.md", "shirousagi-art-revision.md"):
        shutil.copy2(ROOT / "docs/experiments/tasks" / name, frozen / name)
    if args.handoff_project is not None:
        project = args.handoff_project.resolve(strict=True)
        if not project.is_dir() or not (project / "project.kasane.json").is_file():
            raise ValueError("--handoff-project must contain project.kasane.json")
        hashes(project)
        shutil.copytree(project, frozen / "handoff-project")
    if args.shirousagi_root is not None:
        source = args.shirousagi_root.resolve(strict=True)
        required = ("Shirousagi.model3.json", "Shirousagi.moc3", "Shirousagi.psd",
                    "textures/texture_00.png")
        if not source.is_dir() or any(not (source / name).is_file() for name in required):
            raise ValueError("--shirousagi-root must contain model3, MOC3, PSD and texture")
        hashes(source)
        shutil.copytree(source, frozen / "shirousagi", ignore=shutil.ignore_patterns(".DS_Store"))
    shutil.copy2(ROOT / "modules/kasane-python/tests/fixtures/asymmetric-2x2.png", frozen / "texture.png")
    uv = shutil.which(args.uv)
    if not uv:
        raise ValueError("uv not found; install uv before initializing an experiment")
    uv_version = checked([uv, "--version"], root, host / "logs/uv-version").strip()
    python = env_python(root / ".venv")
    checked([uv, "venv", "--python", args.python, str(root / ".venv")], root, host / "logs/venv")
    checked([uv, "pip", "install", "--python", str(python), "--no-deps", "--no-index",
             str(frozen / wheel.name)], root, host / "logs/install")
    runtime = identity(python, root, host / "logs/smoke")
    package = Path(runtime["package"]).parent
    revision = subprocess.run(["git", "rev-parse", "HEAD"], cwd=ROOT, text=True,
                              capture_output=True, check=True).stdout.strip()
    status = subprocess.run(["git", "status", "--porcelain"], cwd=ROOT, text=True,
                            capture_output=True, check=True).stdout
    lock = {"schema_version": 2, "created_at": stamp(), "git_revision": revision,
            "git_status": status, "runtime": runtime, "python": str(python),
            "uv": uv, "uv_version": uv_version, "wheel_name": wheel.name,
            "frozen_hashes": hashes(frozen), "package_hashes": hashes(package)}
    write(host / "lock.json", lock)
    print(json.dumps({"experiment": str(root), "runtime": runtime}, ensure_ascii=False))


def load_experiment(path):
    root = path.resolve(strict=True)
    lock = read(root / "host/lock.json")
    if lock.get("schema_version") != 2:
        raise ValueError("legacy experiment; initialize a new uv experiment")
    if hashes(root / "host/frozen") != lock["frozen_hashes"]:
        raise ValueError("frozen experiment material changed; initialize a new experiment")
    if digest(Path(__file__)) != lock["frozen_hashes"]["sdk_experiments.py"]:
        raise ValueError("experiment runner changed; initialize a new experiment for this runner")
    if hashes(Path(lock["runtime"]["package"]).parent) != lock["package_hashes"]:
        raise ValueError("installed SDK changed; initialize a new experiment")
    return root, lock


def trial_paths(args):
    root, lock = load_experiment(args.experiment)
    if not re.fullmatch(r"[A-Za-z0-9][A-Za-z0-9_-]{0,79}", args.trial):
        raise ValueError("trial ID must be 1-80 ASCII letters, digits, underscores or hyphens")
    return root, lock, root / "host/trials" / args.trial, root / "packets" / args.trial


def worker(root, lock, task, packet, oracle, operation, log, manifest=None, variant=None):
    argv = [lock["python"], "-I", str(root / "host/frozen/worker.py"), operation,
            "--task", task, "--packet", str(packet), "--oracle", str(oracle)]
    if manifest is not None:
        argv += ["--manifest", str(manifest)]
    if variant is not None:
        argv += ["--variant", variant]
    return json.loads(checked(argv, root, log))


def protected_hashes(packet):
    return {"input": hashes(packet / "input"), "docs": hashes(packet / "docs"),
            "task": digest(packet / "TASK.md")}


def prepare(args):
    root, lock, host, packet = trial_paths(args)
    host.mkdir(parents=True, exist_ok=False)
    packet.mkdir(parents=True, exist_ok=False)
    (packet / "input").mkdir()
    (packet / "docs").mkdir()
    frozen = root / "host/frozen"
    for name in ("README.md", "API.md"):
        shutil.copy2(frozen / name, packet / "docs" / name)
    if args.task in ("create", "parameter", "edit"):
        shutil.copy2(frozen / "texture.png", packet / "input/texture.png")
    if args.task == "handoff-revision":
        if not (frozen / "handoff-project").is_dir():
            raise ValueError("handoff-revision requires an experiment initialized with --handoff-project")
        shutil.copytree(frozen / "handoff-project", packet / "input/project")
    if args.task in ("shirousagi-repair", "shirousagi-blink", "shirousagi-art-revision") and not (frozen / "shirousagi").is_dir():
        raise ValueError(f"{args.task} requires --shirousagi-root at init")
    tasks = read(frozen / "tasks.json")["tasks"]
    task = tasks[args.task]
    goal = task["goal"]
    if args.task == "parameter":
        goal = tasks["create"]["goal"].replace("不要添加参数或其他场景对象。", "") + "\n\n" + goal
    # Preparation must pass positive and negative controls before publishing trial.json.
    controls = worker(root, lock, args.task, packet, host / "oracle.json", "prepare", host / "logs/prepare", variant=args.variant)
    if args.task in ("delivery-transfer", "resource-recovery", "visual-locate", "visual-parent", "compose-expression", "handoff-revision", "shirousagi-repair", "shirousagi-blink", "shirousagi-art-revision"):
        template = frozen / f"{args.task}.md"
        instructions = template.read_text(encoding="utf-8")
        instructions += (f"\n## 本轮环境\n\n任务目录：`{packet}`；共享 Python：`{lock['python']}`；"
                         f"预算：{args.budget} 秒。公开文档见 `docs/README.md`、`docs/API.md`。\n"
                         f"先在 `{packet / 'output'}` 运行，脚本接受 `--output` 绝对路径。\n")
    else:
        instructions = f"""# SDK 使用实验：{args.task} v{task['version']}

{goal}

## 环境与边界

工作目录：`{packet}`；本轮实验共用的 Python：`{lock['python']}`。
时间预算 {args.budget} 秒。公开文档在 docs/README.md 和 docs/API.md。
只通过公开 kasane API 创作或编辑工程；不要直接编辑工程 JSON 或使用 _native。
可以检查公开包签名、类型提示，使用 Python 标准库，也允许安装第三方依赖。
使用 `uv pip install --python "{lock['python']}" <包名>` 在共享环境安装依赖。
不要求提交单独的锁文件，底座会记录依赖清单。不要创建新的受试环境。
安装耗时计入实验预算；在 notes.md 说明依赖用途。不得替换、卸载或修改被测 kasane。
不得修改 input/、docs/、TASK.md；不得读取主持人目录、其他任务或源仓库。
工程输出必须位于本任务的 output/ 中；重放时必须使用传入的新输出目录。
数值验收绝对容差为 1e-6。资源路径因保存而迁移是允许的。

## 交付协议

提交本任务目录下的 solution.py；它应接受 `--output <绝对目录>`。
素材位置从 `Path(__file__).resolve().parent / 'input'` 获取，不能依赖旧输出。
脚本必须创建输出目录、显式保存工程，并在输出目录写 result.json：
`{{"project_manifest": "保存结果返回的绝对 manifest 路径"}}`。
请实际运行 `"{lock['python']}" solution.py --output "{packet / 'output'}"`。
可以提交辅助 Python 文件，但它们必须位于本任务目录内。
在 notes.md 记录遇到的错误、恢复方法和仍未解决的问题；不确定的内容请明确标注。
主持人会独立重开交付工程，并复制脚本与输入到新目录，使用同一共享环境重放。
"""
    (packet / "TASK.md").write_text(instructions, encoding="utf-8")
    config = json.loads(args.model_config)
    if not isinstance(config, dict):
        raise ValueError("model-config must be a JSON object")
    record = {"schema_version": 2, "trial": args.trial, "task": args.task,
              "task_version": task["version"], "model": args.model, "model_config": config,
              "variant": args.variant,
              "cohort": args.cohort, "budget_seconds": args.budget, "created_at": stamp(),
              "protected": protected_hashes(packet), "oracle_sha256": digest(host / "oracle.json"),
              "controls": controls, "lock_sha256": digest(root / "host/lock.json")}
    record["initial_environment"] = environment_state(lock, packet)
    write(host / "trial.json", record)
    print(str(packet / "TASK.md"))


def load_trial(args):
    root, lock, host, packet = trial_paths(args)
    record = read(host / "trial.json")
    if record["lock_sha256"] != digest(root / "host/lock.json") or record["oracle_sha256"] != digest(host / "oracle.json"):
        raise ValueError("trial conditions or oracle changed")
    return root, lock, host, packet, record


def run_agent(args):
    root, lock, host, packet, record = load_trial(args)
    if protected_hashes(packet) != record["protected"]:
        raise ValueError("protected task material changed before launch")
    argv = args.command[1:] if args.command[:1] == ["--"] else args.command
    if not argv:
        raise ValueError("provide an agent runner command after --")
    write(host / "environment-before.json", environment_state(lock, packet))
    # One measured agent session per trial; retries belong inside that session.
    result = command(argv, packet, host / "agent", record["budget_seconds"], {
        "KASANE_TASK": str(packet / "TASK.md"), "KASANE_PYTHON": lock["python"],
        "KASANE_UV": lock["uv"], "VIRTUAL_ENV": str(root / ".venv"),
        "UV_PYTHON": lock["python"],
        "PATH": str(Path(lock["python"]).parent) + os.pathsep + os.environ.get("PATH", ""),
        "KASANE_PACKET": str(packet), "KASANE_TRACE": str(packet / "agent-trace.jsonl")})
    write(host / "agent/artifacts.json", packet_hashes(packet))
    try:
        write(host / "agent/environment.json", environment_state(lock, packet))
    except (OSError, ValueError, subprocess.SubprocessError) as exc:
        write(host / "agent/environment.json", {"error": str(exc)})
    print(json.dumps(result, ensure_ascii=False))
    return 0 if result["status"] == "completed" else 1


def result_file(output, task):
    direct = output / "result.json"
    if task not in ("delivery-transfer", "resource-recovery", "visual-locate", "visual-parent", "compose-expression", "handoff-revision", "shirousagi-repair", "shirousagi-blink", "shirousagi-art-revision"):
        return direct
    candidates = list(output.rglob("result.json"))
    if not candidates:
        return direct
    return max(candidates, key=lambda path: (path.stat().st_mtime_ns, str(path)))


def inspect_output(root, lock, record, packet, host, output, log):
    try:
        if output.is_symlink():
            raise ValueError("output must not be a symlink to an existing result")
        hashes(output)  # Reject symlinked artifacts, including links to prior runs.
        result_path = result_file(output, record["task"])
        result = read(result_path)
        raw = Path(result["project_manifest"])
        if not raw.is_absolute():
            raise ValueError("project_manifest must be absolute")
        manifest = raw.resolve(strict=True)
        if not manifest.is_relative_to(output.resolve()):
            raise ValueError("manifest must be inside the declared output directory")
        if record["task"] in ("visual-locate", "visual-parent", "compose-expression", "handoff-revision"):
            argv = [lock["python"], "-I", str(root / "host/frozen/worker.py"), "grade",
                    "--task", record["task"], "--packet", str(packet), "--oracle", str(host / "oracle.json"),
                    "--manifest", str(manifest), "--result", str(result_path)]
            grade = json.loads(checked(argv, root, log))
            grade["result_file"] = str(result_path)
            return grade
        if record["task"] in ("delivery-transfer", "resource-recovery", "shirousagi-repair", "shirousagi-blink", "shirousagi-art-revision"):
            package_raw = Path(result["package_model3"])
            if not package_raw.is_absolute():
                raise ValueError("package_model3 must be absolute")
            package = package_raw.resolve(strict=True)
            if not package.is_relative_to(output.resolve()):
                raise ValueError("package_model3 must be inside the declared output directory")
            argv = [lock["python"], "-I", str(root / "host/frozen/worker.py"), "grade",
                    "--task", record["task"], "--packet", str(packet), "--oracle", str(host / "oracle.json"),
                    "--manifest", str(manifest), "--package", str(package)]
            grade = json.loads(checked(argv, root, log))
            grade["result_file"] = str(result_path)
            return grade
        return worker(root, lock, record["task"], packet, host / "oracle.json", "grade", log, manifest)
    except (ValueError, OSError, KeyError, TypeError) as exc:
        return {"status": "failed", "error": str(exc)}


def assess(args):
    root, lock, host, packet, record = load_trial(args)
    assessment = host / "assessments" / (datetime.now(timezone.utc).strftime("%Y%m%dT%H%M%S") + "-" + uuid4().hex[:8])
    assessment.mkdir(parents=True)
    report = {"schema_version": 1, "created_at": stamp(), "trial": record["trial"], "status": "failed"}
    try:
        report["protected_unchanged"] = protected_hashes(packet) == record["protected"]
        report["submission_hashes"] = packet_hashes(packet)
        report["environment"] = environment_state(lock, packet)
        report["notes_present"] = (packet / "notes.md").is_file() and bool(
            (packet / "notes.md").read_text(encoding="utf-8").strip())
        shutil.copytree(packet, assessment / "submission", ignore=shutil.ignore_patterns("__pycache__", ".venv"))
        report["original"] = inspect_output(root, lock, record, packet, host, packet / "output", assessment / "original")
        replay = assessment / "replay"
        # Reuse the shared environment, but replay from a fresh working/output directory.
        shutil.copytree(packet, replay, ignore=shutil.ignore_patterns("__pycache__", ".venv", "output"))
        report["replay_command"] = command([lock["python"], str(replay / "solution.py"),
            "--output", str(replay / "output")], replay, assessment / "replay-command", args.timeout)
        report["replay_environment"] = environment_state(lock, replay)
        report["environment_unchanged_during_replay"] = report["environment"] == report["replay_environment"]
        report["replay"] = inspect_output(root, lock, record, replay, host, replay / "output", assessment / "replay-grade")
        if record["task"] in ("delivery-transfer", "resource-recovery"):
            previous = read(result_file(replay / "output", record["task"])) if report["replay"]["status"] == "passed" else None
            report["repeat_command"] = command([lock["python"], str(replay / "solution.py"),
                "--output", str(replay / "output")], replay, assessment / "repeat-command", args.timeout)
            report["repeat"] = inspect_output(root, lock, record, replay, host, replay / "output", assessment / "repeat-grade")
            if previous is not None:
                argv = [lock["python"], "-I", str(root / "host/frozen/worker.py"), "grade",
                        "--task", record["task"], "--packet", str(replay),
                        "--oracle", str(host / "oracle.json"), "--manifest", previous["project_manifest"],
                        "--package", previous["package_model3"]]
                report["previous_after_repeat"] = json.loads(checked(argv, root, assessment / "previous-grade"))
            else:
                report["previous_after_repeat"] = {"status": "not_run"}
        report["replay_input_unchanged"] = hashes(replay / "input") == record["protected"]["input"]
        report["protected_after_replay"] = protected_hashes(packet) == record["protected"]
        passed = (report["environment"]["sdk_unchanged"] and report["replay_environment"]["sdk_unchanged"]
                  and report["notes_present"] and report["protected_unchanged"] and report["replay_input_unchanged"]
                  and report["protected_after_replay"] and report["original"]["status"] == "passed"
                  and report["replay"]["status"] == "passed" and report["replay_command"]["status"] == "completed")
        if record["task"] in ("delivery-transfer", "resource-recovery"):
            passed = (passed and report["repeat_command"]["status"] == "completed"
                      and report["repeat"]["status"] == "passed"
                      and report["previous_after_repeat"]["status"] == "passed")
        report["status"] = "passed" if passed else "failed"
    except Exception as exc:
        report.update(status="harness_error", error=f"{type(exc).__name__}: {exc}")
    write(assessment / "report.json", report)
    print(str(assessment / "report.json"))
    return 0 if report["status"] == "passed" else 1


def review(args):
    _, lock, host, packet, _ = load_trial(args)
    path = host / "reviews" / uuid4().hex
    path.mkdir(parents=True)
    trace = args.trace.resolve(strict=True)
    shutil.copy2(trace, path / "trace.log")
    write(path / "review.json", {"created_at": stamp(), "reviewer": args.reviewer,
        "human_prompts": args.human_prompts, "public_api_only": args.public_api == "yes",
        "trace_sha256": digest(path / "trace.log"), "notes": args.notes,
        "submission_hashes": packet_hashes(packet), "environment": environment_state(lock, packet)})
    print(str(path / "review.json"))


def summarize(args):
    root, lock = load_experiment(args.experiment)
    rows = []
    for host in sorted((root / "host/trials").glob("*")):
        if not (host / "trial.json").exists():
            continue
        trial = read(host / "trial.json")
        reports = sorted(host.glob("assessments/*/report.json"), key=lambda p: read(p)["created_at"])
        grade = read(reports[-1]) if reports else None
        reviews = [read(p) for p in host.glob("reviews/*/review.json")]
        matching = [r for r in reviews if grade and r["submission_hashes"] == grade.get("submission_hashes")]
        audited = max(matching, key=lambda r: r["created_at"]) if matching else None
        run = read(host / "agent/command.json") if (host / "agent/command.json").exists() else None
        run_hashes = read(host / "agent/artifacts.json") if (host / "agent/artifacts.json").exists() else None
        run_environment = read(host / "agent/environment.json") if (host / "agent/environment.json").exists() else None
        measured = bool(run and grade and run_hashes == grade.get("submission_hashes")
                        and run_environment == grade.get("environment"))
        independent = None
        if measured and audited and run["status"] != "launch_error" and grade["status"] != "harness_error":
            independent = (run["status"] == "completed" and grade["status"] == "passed"
                           and audited["human_prompts"] == 0 and audited["public_api_only"])
        packet = root / "packets" / trial["trial"]
        current = bool(grade and packet_hashes(packet) == grade.get("submission_hashes"))
        rows.append({"trial": trial["trial"], "task": trial["task"], "task_version": trial["task_version"],
            "model": trial["model"], "model_config": trial["model_config"], "cohort": trial["cohort"],
            "budget_seconds": trial["budget_seconds"], "artifact_status": grade["status"] if grade else "not_run",
            "submission_still_current": current, "measured_run_matches": measured,
            "independent_success": independent if current else None,
            "elapsed_seconds": run["elapsed_seconds"] if run else None,
            "agent_status": run["status"] if run else "not_recorded",
            "assessment_report": str(reports[-1]) if reports else None})
    # Never pool different tasks/models/cohorts into an apparently comparable score.
    result = {"schema_version": 1, "experiment_lock_sha256": digest(root / "host/lock.json"), "trials": rows}
    if args.output:
        write(args.output.resolve(), result)
    print(json.dumps(result, indent=2, ensure_ascii=False))


def positive_int(value):
    number = int(value)
    if number <= 0:
        raise argparse.ArgumentTypeError("must be positive")
    return number


def nonnegative_int(value):
    number = int(value)
    if number < 0:
        raise argparse.ArgumentTypeError("must be nonnegative")
    return number


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    sub = parser.add_subparsers(dest="action", required=True)
    for name, handler in [("init", initialize), ("prepare", prepare), ("run", run_agent),
                          ("assess", assess), ("review", review), ("summarize", summarize)]:
        item = sub.add_parser(name)
        item.add_argument("--experiment", type=Path, required=True)
        item.set_defaults(handler=handler)
        if name not in ("init", "summarize"):
            item.add_argument("--trial", required=True)
        if name == "init":
            item.add_argument("--wheel", type=Path, required=True)
            item.add_argument("--handoff-project", type=Path)
            item.add_argument("--shirousagi-root", type=Path)
            item.add_argument("--python", default="3.14", help="uv Python version or interpreter path")
            item.add_argument("--uv", default="uv")
        elif name == "prepare":
            item.add_argument("--task", choices=["create", "parameter", "edit", "delivery-transfer", "resource-recovery", "visual-locate", "visual-parent", "compose-expression", "handoff-revision", "shirousagi-repair", "shirousagi-blink", "shirousagi-art-revision"], required=True)
            item.add_argument("--variant", choices=["a", "b", "c"], default="a")
            item.add_argument("--model", required=True, help="Exact model identifier, not a nickname")
            item.add_argument("--model-config", default="{}", help="JSON: reasoning, sampling, harness version, etc.")
            item.add_argument("--cohort", choices=["fresh", "learning"], default="fresh")
            item.add_argument("--budget", type=positive_int, default=900)
        elif name == "run":
            item.add_argument("command", nargs=argparse.REMAINDER)
        elif name == "assess":
            item.add_argument("--timeout", type=positive_int, default=180)
        elif name == "review":
            item.add_argument("--reviewer", required=True)
            item.add_argument("--human-prompts", type=nonnegative_int, required=True)
            item.add_argument("--public-api", choices=["yes", "no"], required=True)
            item.add_argument("--trace", type=Path, required=True)
            item.add_argument("--notes", required=True)
        elif name == "summarize":
            item.add_argument("--output", type=Path)
    args = parser.parse_args()
    try:
        return args.handler(args) or 0
    except (ValueError, OSError, RuntimeError, subprocess.SubprocessError) as exc:
        print(f"error: {exc}", file=sys.stderr)
        return 2


if __name__ == "__main__":
    raise SystemExit(main())

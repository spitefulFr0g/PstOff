#!/usr/bin/env python3
"""Offline build in a network/home-free sandbox (including build scripts)."""
import json,subprocess
from pathlib import Path
base=Path('/home/watermelon/.local/share/pstoff-spike22')
roots=json.loads((base/'sources.json').read_text())
toolchain=Path(subprocess.check_output(['/home/watermelon/.cargo/bin/rustc','--print','sysroot'],text=True).strip())
(base/'target').mkdir(exist_ok=True)
cmd=['bwrap','--unshare-all','--die-with-parent','--new-session','--ro-bind','/usr','/usr','--ro-bind','/lib','/lib','--ro-bind','/lib64','/lib64','--ro-bind','/etc/alternatives','/etc/alternatives','--proc','/proc','--dev','/dev','--tmpfs','/tmp','--ro-bind',str(toolchain),'/toolchain','--ro-bind',str(Path.home()/'.cargo/registry'),'/cargo/registry','--ro-bind',str(base/'source'),str(base/'source'),'--ro-bind',roots['microsoft'],roots['microsoft'],'--bind',str(base/'target'),'/target','--ro-bind',str(Path(__file__).resolve().parent/'probe'),'/probe','--clearenv','--setenv','PATH','/toolchain/bin:/usr/bin','--setenv','CARGO_HOME','/cargo','--setenv','LD_LIBRARY_PATH','/toolchain/lib','--chdir','/probe','/toolchain/bin/cargo','build','--offline','--locked','--release','--manifest-path','/probe/Cargo.toml','--target-dir','/target']
subprocess.run(cmd,check=True)

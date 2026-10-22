# assets/icon/reframe-1024.png 에서 플랫폼별 앱 아이콘을 만든다. (Pillow 필요: pip install pillow)
#
# - reframe.ico   : Windows 실행 파일 아이콘 (build.rs 가 리소스로 넣는다)
# - reframe.icns  : macOS .app 아이콘 (scripts/package-macos.sh)
# - window-64.rgba: 실행 중 창 아이콘 (main.rs 가 그대로 읽는 64×64 RGBA)
# - reframe-256.png: 앱 정보 창 로고
#
# 원본 그림은 assets/icon/reframe.prism (Prism 문서)이고, 고친 뒤 reframe-1024.png 로 내보내 이 스크립트를 돌린다.
# 실행: python scripts/make-icons.py
from pathlib import Path

from PIL import Image

DIR = Path(__file__).resolve().parent.parent / "assets" / "icon"
src = Image.open(DIR / "reframe-1024.png").convert("RGBA")

src.save(DIR / "reframe.ico", sizes=[(s, s) for s in (16, 20, 24, 32, 40, 48, 64, 128, 256)])
src.save(DIR / "reframe.icns")
(DIR / "window-64.rgba").write_bytes(src.resize((64, 64), Image.LANCZOS).tobytes())
src.resize((256, 256), Image.LANCZOS).save(DIR / "reframe-256.png", optimize=True)
print("icons written to", DIR)

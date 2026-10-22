// Windows 실행 파일에 앱 아이콘(assets/icon/reframe.ico)을 리소스로 넣는다.
// 탐색기·작업 표시줄·바로 가기에 이 아이콘이 쓰인다. 다른 OS 에서는 아무것도 하지 않는다.
fn main() {
    println!("cargo:rerun-if-changed=assets/icon/reframe.ico");
    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() == Ok("windows") {
        let mut res = winresource::WindowsResource::new();
        res.set_icon("assets/icon/reframe.ico");
        res.compile().expect("Windows 리소스(아이콘) 컴파일 실패");
    }
}

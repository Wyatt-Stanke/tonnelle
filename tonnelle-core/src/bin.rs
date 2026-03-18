#[tokio::main]
async fn main() {}
fn test() {
    let s = std::net::TcpStream::connect("1.1.1.1:80").unwrap();
    let _ = tokio::net::TcpSocket::from_std_stream(s);
}

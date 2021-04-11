FROM --platform=linux/arm64 debian:10.8
ADD "https://www.random.org/cgi-bin/randbyte?nbytes=10&format=h" nocache
ADD target/aarch64-unknown-linux-gnu/debug/poodle /bin/poodle
ENTRYPOINT ["poodle"]

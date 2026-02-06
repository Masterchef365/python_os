cargo bootimage --release &&\
    qemu-system-x86_64 -enable-kvm -cpu host -drive format=raw,file=target/x86_64-blog_os/release/bootimage-python_os.bin

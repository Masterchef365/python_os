cargo bootimage --release &&\
    qemu-system-x86_64 --enable-kvm -drive format=raw,file=target/x86_64-blog_os/release/bootimage-python_os.bin

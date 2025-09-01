cargo bootimage &&\
    alacritty --working-directory $PWD -e qemu-system-x86_64 -s -S -drive format=raw,file=target/x86_64-blog_os/debug/bootimage-python_os.bin &

gdb -ex 'target remote localhost:1234' ./target/x86_64-blog_os/debug/python_os


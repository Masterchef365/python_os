if ./build.sh; then
    echo "BUILD OK"
else
    exit
fi
alacritty --working-directory $PWD -e qemu-system-x86_64 -enable-kvm -cpu host -s -S -drive format=raw,file=target/x86_64-blog_os/debug/bootimage-python_os.bin &

gdb\
    -ex 'target remote localhost:1234'\
    -ex 'b _start'\
    ./target/x86_64-blog_os/debug/python_os

#-ex 'b _start'\
#-ex 'b src/main.rs:50'\

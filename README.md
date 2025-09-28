# ext4-image-writer
An ext4 image writing library written in Rust.

# What?
Create ext4 images that contain a tree of files. This is often needed when building images for embedded devices running Linux.

# Why not simply use `mkfs.ext4` from `e2fsprogs`?
* No need to have the files on disk first.  
  This allows building quicker tools in some contexts (and tools that run on non-Unix platforms like Windows). 
* No need to know the filesystem size in advance.  
  The superblock and other size-dependent data structures are only written in the end when all files have been written. This allows you to create minimally sized images. 

# FIO

job

```
[global]
bs=4K
iodepth=256
direct=1
ioengine=io_uring
group_reporting
time_based
runtime=120
numjobs=4
name=raw-randreadwrite
rw=randrw

[job1]
filename=test.fio.file
size=1g
```

## SSD SPEED

Running the above job on SSD directly gave us the following results

```
job1: (g=0): rw=randrw, bs=(R) 4096B-4096B, (W) 4096B-4096B, (T) 4096B-4096B, ioengine=io_uring, iodepth=256
...
fio-3.36-17-gafdde5
Starting 4 processes
job1: Laying out IO file (1 file / 1024MiB)
Jobs: 4 (f=4): [m(4)][100.0%][r=59.4MiB/s,w=58.6MiB/s][r=15.2k,w=15.0k IOPS][eta 00m:00s]
job1: (groupid=0, jobs=4): err= 0: pid=795: Thu Nov 16 08:35:05 2023
  read: IOPS=15.3k, BW=59.7MiB/s (62.6MB/s)(7174MiB/120186msec)
    slat (usec): min=10, max=3415.2k, avg=244.50, stdev=8911.23
    clat (usec): min=52, max=3749.7k, avg=29280.46, stdev=106887.36
     lat (usec): min=134, max=3749.7k, avg=29524.96, stdev=107414.20
    clat percentiles (usec):
     |  1.00th=[    465],  5.00th=[   2900], 10.00th=[  10421],
     | 20.00th=[  15008], 30.00th=[  16909], 40.00th=[  17957],
     | 50.00th=[  19268], 60.00th=[  20317], 70.00th=[  21627],
     | 80.00th=[  23462], 90.00th=[  26608], 95.00th=[  36963],
     | 99.00th=[ 223347], 99.50th=[ 375391], 99.90th=[1568670],
     | 99.95th=[3238003], 99.99th=[3674211]
   bw (  KiB/s): min=  120, max=116248, per=100.00%, avg=66769.01, stdev=9564.22, samples=880
   iops        : min=   30, max=29062, avg=16692.26, stdev=2391.06, samples=880
  write: IOPS=15.3k, BW=59.7MiB/s (62.6MB/s)(7175MiB/120186msec); 0 zone resets
    slat (nsec): min=1055, max=6977.8k, avg=5005.10, stdev=10233.03
    clat (msec): min=2, max=3755, avg=37.47, stdev=117.48
     lat (msec): min=2, max=3755, avg=37.47, stdev=117.48
    clat percentiles (msec):
     |  1.00th=[   13],  5.00th=[   16], 10.00th=[   17], 20.00th=[   19],
     | 30.00th=[   20], 40.00th=[   22], 50.00th=[   23], 60.00th=[   24],
     | 70.00th=[   26], 80.00th=[   29], 90.00th=[   36], 95.00th=[   94],
     | 99.00th=[  275], 99.50th=[  477], 99.90th=[ 1586], 99.95th=[ 3239],
     | 99.99th=[ 3708]
   bw (  KiB/s): min=  152, max=120008, per=100.00%, avg=66622.51, stdev=9615.30, samples=882
   iops        : min=   38, max=30002, avg=16655.62, stdev=2403.82, samples=882
  lat (usec)   : 100=0.01%, 250=0.19%, 500=0.35%, 750=0.33%, 1000=0.29%
  lat (msec)   : 2=0.86%, 4=0.92%, 10=2.01%, 20=39.35%, 50=50.51%
  lat (msec)   : 100=0.80%, 250=3.34%, 500=0.61%, 750=0.20%, 1000=0.07%
  lat (msec)   : 2000=0.07%, >=2000=0.08%
  cpu          : usr=3.68%, sys=15.45%, ctx=918536, majf=0, minf=19667
  IO depths    : 1=0.1%, 2=0.1%, 4=0.1%, 8=0.1%, 16=0.1%, 32=0.1%, >=64=100.0%
     submit    : 0=0.0%, 4=100.0%, 8=0.0%, 16=0.0%, 32=0.0%, 64=0.0%, >=64=0.0%
     complete  : 0=0.0%, 4=100.0%, 8=0.0%, 16=0.0%, 32=0.0%, 64=0.0%, >=64=0.1%
     issued rwts: total=1836609,1836769,0,0 short=0,0,0,0 dropped=0,0,0,0
     latency   : target=0, window=0, percentile=100.00%, depth=256

Run status group 0 (all jobs):
   READ: bw=59.7MiB/s (62.6MB/s), 59.7MiB/s-59.7MiB/s (62.6MB/s-62.6MB/s), io=7174MiB (7523MB), run=120186-120186msec
  WRITE: bw=59.7MiB/s (62.6MB/s), 59.7MiB/s-59.7MiB/s (62.6MB/s-62.6MB/s), io=7175MiB (7523MB), run=120186-120186msec

```

## HDD

Running the above job on HDD directly gave us the following results

```
job1: (g=0): rw=randrw, bs=(R) 4096B-4096B, (W) 4096B-4096B, (T) 4096B-4096B, ioengine=io_uring, iodepth=256
...
fio-3.36-17-gafdde5
Starting 4 processes
job1: Laying out IO file (1 file / 1024MiB)
Jobs: 4 (f=4): [m(4)][100.0%][r=1440KiB/s,w=1352KiB/s][r=360,w=338 IOPS][eta 00m:00s]
job1: (groupid=0, jobs=4): err= 0: pid=12669: Thu Nov 16 08:39:29 2023
  read: IOPS=334, BW=1340KiB/s (1372kB/s)(157MiB/120213msec)
    slat (usec): min=12, max=277785, avg=11903.34, stdev=22219.51
    clat (msec): min=171, max=3100, avg=1457.17, stdev=277.72
     lat (msec): min=229, max=3100, avg=1469.08, stdev=279.32
    clat percentiles (msec):
     |  1.00th=[  936],  5.00th=[ 1083], 10.00th=[ 1150], 20.00th=[ 1250],
     | 30.00th=[ 1318], 40.00th=[ 1368], 50.00th=[ 1435], 60.00th=[ 1485],
     | 70.00th=[ 1552], 80.00th=[ 1653], 90.00th=[ 1787], 95.00th=[ 1955],
     | 99.00th=[ 2333], 99.50th=[ 2500], 99.90th=[ 2769], 99.95th=[ 2802],
     | 99.99th=[ 2970]
   bw (  KiB/s): min=  248, max= 2554, per=99.71%, avg=1336.87, stdev=100.68, samples=952
   iops        : min=   62, max=  638, avg=334.21, stdev=25.17, samples=952
  write: IOPS=337, BW=1348KiB/s (1380kB/s)(158MiB/120213msec); 0 zone resets
    slat (nsec): min=1364, max=246481, avg=8240.34, stdev=6664.73
    clat (msec): min=229, max=3200, avg=1563.82, stdev=281.29
     lat (msec): min=229, max=3200, avg=1563.82, stdev=281.29
    clat percentiles (msec):
     |  1.00th=[ 1020],  5.00th=[ 1183], 10.00th=[ 1267], 20.00th=[ 1351],
     | 30.00th=[ 1418], 40.00th=[ 1469], 50.00th=[ 1536], 60.00th=[ 1603],
     | 70.00th=[ 1670], 80.00th=[ 1754], 90.00th=[ 1905], 95.00th=[ 2056],
     | 99.00th=[ 2433], 99.50th=[ 2567], 99.90th=[ 2869], 99.95th=[ 2903],
     | 99.99th=[ 3037]
   bw (  KiB/s): min=  144, max= 2720, per=99.70%, avg=1344.10, stdev=99.26, samples=952
   iops        : min=   36, max=  680, avg=336.02, stdev=24.81, samples=952
  lat (msec)   : 250=0.03%, 500=0.22%, 750=0.21%, 1000=1.04%, 2000=93.10%
  lat (msec)   : >=2000=5.39%
  cpu          : usr=0.16%, sys=1.17%, ctx=26320, majf=0, minf=947
  IO depths    : 1=0.1%, 2=0.1%, 4=0.1%, 8=0.1%, 16=0.1%, 32=0.2%, >=64=99.7%
     submit    : 0=0.0%, 4=100.0%, 8=0.0%, 16=0.0%, 32=0.0%, 64=0.0%, >=64=0.0%
     complete  : 0=0.0%, 4=100.0%, 8=0.0%, 16=0.0%, 32=0.0%, 64=0.0%, >=64=0.1%
     issued rwts: total=40266,40514,0,0 short=0,0,0,0 dropped=0,0,0,0
     latency   : target=0, window=0, percentile=100.00%, depth=256

Run status group 0 (all jobs):
   READ: bw=1340KiB/s (1372kB/s), 1340KiB/s-1340KiB/s (1372kB/s-1372kB/s), io=157MiB (165MB), run=120213-120213msec
  WRITE: bw=1348KiB/s (1380kB/s), 1348KiB/s-1348KiB/s (1380kB/s-1380kB/s), io=158MiB (166MB), run=120213-120213msec
```

## NBD (5GiB cache)

The device is set to use 5gib cache, and a page-size of 256kib. (so the cache is bigger than the file used)
(the total device size is 200G)

```
job1: (g=0): rw=randrw, bs=(R) 4096B-4096B, (W) 4096B-4096B, (T) 4096B-4096B, ioengine=io_uring, iodepth=256
...
fio-3.36-17-gafdde5
Starting 4 processes
job1: Laying out IO file (1 file / 1024MiB)
Jobs: 4 (f=4): [m(4)][100.0%][r=6622KiB/s,w=6122KiB/s][r=1655,w=1530 IOPS][eta 00m:00s]
job1: (groupid=0, jobs=4): err= 0: pid=9962: Thu Nov 16 09:02:18 2023
  read: IOPS=4580, BW=17.9MiB/s (18.8MB/s)(2149MiB/120107msec)
    slat (usec): min=11, max=746398, avg=857.98, stdev=4621.29
    clat (msec): min=2, max=1977, avg=109.96, stdev=120.86
     lat (msec): min=2, max=2005, avg=110.82, stdev=121.80
    clat percentiles (msec):
     |  1.00th=[   15],  5.00th=[   20], 10.00th=[   23], 20.00th=[   28],
     | 30.00th=[   32], 40.00th=[   38], 50.00th=[   55], 60.00th=[   78],
     | 70.00th=[  144], 80.00th=[  226], 90.00th=[  268], 95.00th=[  300],
     | 99.00th=[  401], 99.50th=[  485], 99.90th=[ 1301], 99.95th=[ 1502],
     | 99.99th=[ 1854]
   bw (  KiB/s): min=  456, max=80792, per=100.00%, avg=18323.30, stdev=5196.11, samples=960
   iops        : min=  114, max=20198, avg=4580.83, stdev=1299.03, samples=960
  write: IOPS=4587, BW=17.9MiB/s (18.8MB/s)(2152MiB/120107msec); 0 zone resets
    slat (nsec): min=1187, max=1216.0k, avg=5814.05, stdev=7088.99
    clat (msec): min=4, max=3222, avg=112.43, stdev=129.84
     lat (msec): min=4, max=3222, avg=112.43, stdev=129.84
    clat percentiles (msec):
     |  1.00th=[   15],  5.00th=[   20], 10.00th=[   23], 20.00th=[   27],
     | 30.00th=[   30], 40.00th=[   36], 50.00th=[   52], 60.00th=[   73],
     | 70.00th=[  153], 80.00th=[  234], 90.00th=[  279], 95.00th=[  317],
     | 99.00th=[  422], 99.50th=[  518], 99.90th=[ 1418], 99.95th=[ 1653],
     | 99.99th=[ 1921]
   bw (  KiB/s): min=  608, max=80424, per=99.99%, avg=18350.10, stdev=5191.81, samples=960
   iops        : min=  152, max=20106, avg=4587.52, stdev=1297.95, samples=960
  lat (msec)   : 4=0.01%, 10=0.19%, 20=5.92%, 50=42.76%, 100=18.44%
  lat (msec)   : 250=17.82%, 500=14.36%, 750=0.16%, 1000=0.09%, 2000=0.25%
  lat (msec)   : >=2000=0.01%
  cpu          : usr=1.34%, sys=5.56%, ctx=354650, majf=0, minf=7614
  IO depths    : 1=0.1%, 2=0.1%, 4=0.1%, 8=0.1%, 16=0.1%, 32=0.1%, >=64=100.0%
     submit    : 0=0.0%, 4=100.0%, 8=0.0%, 16=0.0%, 32=0.0%, 64=0.0%, >=64=0.0%
     complete  : 0=0.0%, 4=100.0%, 8=0.0%, 16=0.0%, 32=0.0%, 64=0.0%, >=64=0.1%
     issued rwts: total=550191,551031,0,0 short=0,0,0,0 dropped=0,0,0,0
     latency   : target=0, window=0, percentile=100.00%, depth=256

Run status group 0 (all jobs):
   READ: bw=17.9MiB/s (18.8MB/s), 17.9MiB/s-17.9MiB/s (18.8MB/s-18.8MB/s), io=2149MiB (2254MB), run=120107-120107msec
  WRITE: bw=17.9MiB/s (18.8MB/s), 17.9MiB/s-17.9MiB/s (18.8MB/s-18.8MB/s), io=2152MiB (2257MB), run=120107-120107msec
```

## NBD (1GiB cache)

The device is set to use 1gib cache, and a page-size of 256kib.
(the total device size is 4G)

```
job1: (g=0): rw=randrw, bs=(R) 4096B-4096B, (W) 4096B-4096B, (T) 4096B-4096B, ioengine=io_uring, iodepth=256
...
fio-3.36-17-gafdde5
Starting 4 processes
job1: Laying out IO file (1 file / 1024MiB)
Jobs: 4 (f=4): [m(4)][6.0%][r=4KiB/s,w=12KiB/s][r=1,w=3 IOPS][eta 31m:37s]
job1: (groupid=0, jobs=4): err= 0: pid=23800: Thu Nov 16 09:12:07 2023
  read: IOPS=266, BW=1068KiB/s (1093kB/s)(127MiB/121552msec)
    slat (usec): min=11, max=1814.7k, avg=14797.58, stdev=58231.71
    clat (msec): min=112, max=6131, avg=1963.01, stdev=944.90
     lat (msec): min=112, max=6132, avg=1977.81, stdev=950.21
    clat percentiles (msec):
     |  1.00th=[  405],  5.00th=[  575], 10.00th=[  726], 20.00th=[ 1099],
     | 30.00th=[ 1368], 40.00th=[ 1620], 50.00th=[ 1871], 60.00th=[ 2198],
     | 70.00th=[ 2467], 80.00th=[ 2769], 90.00th=[ 3239], 95.00th=[ 3608],
     | 99.00th=[ 4396], 99.50th=[ 4732], 99.90th=[ 5403], 99.95th=[ 5671],
     | 99.99th=[ 6141]
   bw (  KiB/s): min=   32, max= 4872, per=100.00%, avg=1166.98, stdev=204.62, samples=875
   iops        : min=    8, max= 1218, avg=291.74, stdev=51.16, samples=875
  write: IOPS=269, BW=1076KiB/s (1102kB/s)(128MiB/121552msec); 0 zone resets
    slat (nsec): min=1238, max=331477, avg=5938.66, stdev=5159.34
    clat (msec): min=112, max=5481, avg=1837.42, stdev=892.62
     lat (msec): min=112, max=5481, avg=1837.43, stdev=892.62
    clat percentiles (msec):
     |  1.00th=[  376],  5.00th=[  542], 10.00th=[  651], 20.00th=[  995],
     | 30.00th=[ 1250], 40.00th=[ 1519], 50.00th=[ 1770], 60.00th=[ 2056],
     | 70.00th=[ 2333], 80.00th=[ 2601], 90.00th=[ 3037], 95.00th=[ 3373],
     | 99.00th=[ 4010], 99.50th=[ 4178], 99.90th=[ 4665], 99.95th=[ 4732],
     | 99.99th=[ 5067]
   bw (  KiB/s): min=   32, max= 4680, per=100.00%, avg=1179.88, stdev=206.92, samples=874
   iops        : min=    8, max= 1170, avg=294.97, stdev=51.73, samples=874
  lat (msec)   : 250=0.05%, 500=2.97%, 750=9.00%, 1000=6.74%, 2000=37.40%
  lat (msec)   : >=2000=43.84%
  cpu          : usr=0.08%, sys=0.60%, ctx=21212, majf=0, minf=37
  IO depths    : 1=0.1%, 2=0.1%, 4=0.1%, 8=0.1%, 16=0.1%, 32=0.2%, >=64=99.6%
     submit    : 0=0.0%, 4=100.0%, 8=0.0%, 16=0.0%, 32=0.0%, 64=0.0%, >=64=0.0%
     complete  : 0=0.0%, 4=100.0%, 8=0.0%, 16=0.0%, 32=0.0%, 64=0.0%, >=64=0.1%
     issued rwts: total=32442,32706,0,0 short=0,0,0,0 dropped=0,0,0,0
     latency   : target=0, window=0, percentile=100.00%, depth=256

Run status group 0 (all jobs):
   READ: bw=1068KiB/s (1093kB/s), 1068KiB/s-1068KiB/s (1093kB/s-1093kB/s), io=127MiB (133MB), run=121552-121552msec
  WRITE: bw=1076KiB/s (1102kB/s), 1076KiB/s-1076KiB/s (1102kB/s-1102kB/s), io=128MiB (134MB), run=121552-121552msec
```

## NBD (1GiB cache) again

This runs the same test as above except:

- Test file is 1.5Gib (bigger than cache can hold)
- Cache is already fully allocated because of the previous test

The device is set to use 1gib cache, and a page-size of 256kib.
(the total device size is 4G)

> This is why the choice of the cache size is important it need to be able to hold the avg file size of `hot` files.

```
job1: (g=0): rw=randrw, bs=(R) 4096B-4096B, (W) 4096B-4096B, (T) 4096B-4096B, ioengine=io_uring, iodepth=256
...
fio-3.36-17-gafdde5
Starting 4 processes
job1: Laying out IO file (1 file / 1500MiB)
Jobs: 4 (f=4): [m(4)][0.9%][eta 03h:33m:18s]
job1: (groupid=0, jobs=4): err= 0: pid=2455: Thu Nov 16 09:20:29 2023
  read: IOPS=64, BW=259KiB/s (265kB/s)(31.0MiB/122609msec)
    slat (usec): min=2, max=2424.4k, avg=60171.41, stdev=169295.69
    clat (msec): min=1334, max=13799, avg=7848.03, stdev=2013.21
     lat (msec): min=1334, max=13799, avg=7908.21, stdev=2024.00
    clat percentiles (msec):
     |  1.00th=[ 3205],  5.00th=[ 4463], 10.00th=[ 5269], 20.00th=[ 6208],
     | 30.00th=[ 6678], 40.00th=[ 7282], 50.00th=[ 7819], 60.00th=[ 8356],
     | 70.00th=[ 8926], 80.00th=[ 9597], 90.00th=[10537], 95.00th=[11208],
     | 99.00th=[12416], 99.50th=[12818], 99.90th=[13624], 99.95th=[13624],
     | 99.99th=[13758]
   bw (  KiB/s): min=   32, max= 2008, per=100.00%, avg=375.17, stdev=81.20, samples=636
   iops        : min=    8, max=  502, avg=93.79, stdev=20.30, samples=636
  write: IOPS=67, BW=271KiB/s (277kB/s)(32.4MiB/122609msec); 0 zone resets
    slat (nsec): min=1205, max=267920, avg=6208.72, stdev=6846.11
    clat (msec): min=1878, max=13645, avg=7353.41, stdev=1889.20
     lat (msec): min=1878, max=13645, avg=7353.41, stdev=1889.19
    clat percentiles (msec):
     |  1.00th=[ 3104],  5.00th=[ 4329], 10.00th=[ 4799], 20.00th=[ 5738],
     | 30.00th=[ 6409], 40.00th=[ 6812], 50.00th=[ 7349], 60.00th=[ 7886],
     | 70.00th=[ 8288], 80.00th=[ 8926], 90.00th=[ 9731], 95.00th=[10537],
     | 99.00th=[11745], 99.50th=[12147], 99.90th=[13087], 99.95th=[13221],
     | 99.99th=[13624]
   bw (  KiB/s): min=   32, max= 2040, per=100.00%, avg=374.49, stdev=79.41, samples=664
   iops        : min=    8, max=  510, avg=93.62, stdev=19.85, samples=664
  lat (msec)   : 2000=0.10%, >=2000=99.90%
  cpu          : usr=0.02%, sys=0.81%, ctx=5760, majf=0, minf=829
  IO depths    : 1=0.1%, 2=0.1%, 4=0.1%, 8=0.2%, 16=0.4%, 32=0.8%, >=64=98.4%
     submit    : 0=0.0%, 4=100.0%, 8=0.0%, 16=0.0%, 32=0.0%, 64=0.0%, >=64=0.0%
     complete  : 0=0.0%, 4=100.0%, 8=0.0%, 16=0.0%, 32=0.0%, 64=0.0%, >=64=0.1%
     issued rwts: total=7934,8297,0,0 short=0,0,0,0 dropped=0,0,0,0
     latency   : target=0, window=0, percentile=100.00%, depth=256

Run status group 0 (all jobs):
   READ: bw=259KiB/s (265kB/s), 259KiB/s-259KiB/s (265kB/s-265kB/s), io=31.0MiB (32.5MB), run=122609-122609msec
  WRITE: bw=271KiB/s (277kB/s), 271KiB/s-271KiB/s (277kB/s-277kB/s), io=32.4MiB (34.0MB), run=122609-122609msec
```

Running same test (dirty cache) but only a 500Mib test file (random read/write)

```
job1: (g=0): rw=randrw, bs=(R) 4096B-4096B, (W) 4096B-4096B, (T) 4096B-4096B, ioengine=io_uring, iodepth=256
...
fio-3.36-17-gafdde5
Starting 4 processes
Jobs: 4 (f=4): [m(4)][100.0%][r=4304KiB/s,w=4172KiB/s][r=1076,w=1043 IOPS][eta 00m:00s]
job1: (groupid=0, jobs=4): err= 0: pid=8840: Thu Nov 16 09:24:50 2023
  read: IOPS=1774, BW=7099KiB/s (7269kB/s)(834MiB/120375msec)
    slat (usec): min=2, max=2086.0k, avg=2232.98, stdev=16594.41
    clat (msec): min=6, max=3324, avg=289.70, stdev=237.04
     lat (msec): min=6, max=3324, avg=291.94, stdev=238.31
    clat percentiles (msec):
     |  1.00th=[   45],  5.00th=[   77], 10.00th=[  109], 20.00th=[  155],
     | 30.00th=[  190], 40.00th=[  222], 50.00th=[  249], 60.00th=[  275],
     | 70.00th=[  317], 80.00th=[  376], 90.00th=[  485], 95.00th=[  600],
     | 99.00th=[ 1045], 99.50th=[ 1653], 99.90th=[ 2903], 99.95th=[ 3004],
     | 99.99th=[ 3071]
   bw (  KiB/s): min=  240, max=22904, per=100.00%, avg=7278.64, stdev=941.99, samples=937
   iops        : min=   60, max= 5726, avg=1819.66, stdev=235.50, samples=937
  write: IOPS=1776, BW=7104KiB/s (7275kB/s)(835MiB/120375msec); 0 zone resets
    slat (nsec): min=1224, max=762266, avg=5424.65, stdev=5162.91
    clat (msec): min=9, max=3324, avg=283.83, stdev=236.24
     lat (msec): min=9, max=3324, avg=283.83, stdev=236.24
    clat percentiles (msec):
     |  1.00th=[   42],  5.00th=[   70], 10.00th=[   99], 20.00th=[  144],
     | 30.00th=[  186], 40.00th=[  220], 50.00th=[  247], 60.00th=[  275],
     | 70.00th=[  309], 80.00th=[  368], 90.00th=[  477], 95.00th=[  592],
     | 99.00th=[ 1036], 99.50th=[ 1536], 99.90th=[ 2937], 99.95th=[ 3004],
     | 99.99th=[ 3205]
   bw (  KiB/s): min=  104, max=22648, per=100.00%, avg=7275.88, stdev=941.10, samples=938
   iops        : min=   26, max= 5662, avg=1818.97, stdev=235.27, samples=938
  lat (msec)   : 10=0.01%, 20=0.02%, 50=1.53%, 100=7.95%, 250=41.66%
  lat (msec)   : 500=39.94%, 750=6.57%, 1000=1.14%, 2000=0.72%, >=2000=0.45%
  cpu          : usr=0.50%, sys=2.18%, ctx=144765, majf=0, minf=2970
  IO depths    : 1=0.1%, 2=0.1%, 4=0.1%, 8=0.1%, 16=0.1%, 32=0.1%, >=64=99.9%
     submit    : 0=0.0%, 4=100.0%, 8=0.0%, 16=0.0%, 32=0.0%, 64=0.0%, >=64=0.0%
     complete  : 0=0.0%, 4=100.0%, 8=0.0%, 16=0.0%, 32=0.0%, 64=0.0%, >=64=0.1%
     issued rwts: total=213624,213792,0,0 short=0,0,0,0 dropped=0,0,0,0
     latency   : target=0, window=0, percentile=100.00%, depth=256

Run status group 0 (all jobs):
   READ: bw=7099KiB/s (7269kB/s), 7099KiB/s-7099KiB/s (7269kB/s-7269kB/s), io=834MiB (875MB), run=120375-120375msec
  WRITE: bw=7104KiB/s (7275kB/s), 7104KiB/s-7104KiB/s (7275kB/s-7275kB/s), io=835MiB (876MB), run=120375-120375msec
```

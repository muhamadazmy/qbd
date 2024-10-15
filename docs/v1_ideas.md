# Ideas for a production release of QBD

## first some questions

- can backend be in multiple files e.g. 4MB each, or configurable

## implementation

- we should support erasure coding or replication over multiple files
- we should support remote NBD devices as backend


```mermaid
flowchart TB
    %% Subgraph for Computer 1
    subgraph Computer1
        C1MasterNBD[Master NBD Volume<br>does caching in mem, configurable]
        C1SlaveNBD[Back NBD Volume 1]
    end

    %% Subgraph for Computer 2
    subgraph Computer2
        C2SlaveNBD1[Back NBD Volume 2]
    
    end

    %% Subgraph for Computer 3
    subgraph Computer3
        C3SlaveNBD[Back NBD Volume 3]
        C3SlaveNBD --> SSD1[4MB Files on NVME, can act as cache or primary]
        C3SlaveNBD --> HDD1[4MB Files on HDD, optional, backend disk slow]
    end

    %% Connections between the master and slave NBDs
    C1MasterNBD -- TCP --> C1SlaveNBD
    C1MasterNBD -- TCP --> C2SlaveNBD1
    C1MasterNBD -- TCP --> C3SlaveNBD
```

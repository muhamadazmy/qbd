
## MagicCloud Edge Cluster


```mermaid
flowchart TB
    %% Cluster 1
    subgraph Cluster1 [**Cluster 1**]
        direction LR
        C1_Switch[Integrated Secure<br>Internet Backplane]
        
        C1_Node1[Node 1] --- C1_Switch
        C1_Node2[Node 2] --- C1_Switch
        C1_Node3[Node 3] --- C1_Switch
        C1_Node4[Node 4] --- C1_Switch
        C1_Node5[Node 5] --- C1_Switch

        %% 40 Gbit Backplane in Circular Topology
        C1_Node1 ---|40 Gbit| C1_Node2
        C1_Node2 ---|40 Gbit| C1_Node3
        C1_Node3 ---|40 Gbit| C1_Node4
        C1_Node4 ---|40 Gbit| C1_Node5
        C1_Node5 ---|40 Gbit| C1_Node1
    end
    
    %% Cluster 2
    subgraph Cluster2 [**Cluster 2**]
        direction LR
        C2_Switch[Integrated Secure<br>Internet Backplane]
        
        C2_Node1[Node 1] --- C2_Switch
        C2_Node2[Node 2] --- C2_Switch
        C2_Node3[Node 3] --- C2_Switch
        C2_Node4[Node 4] --- C2_Switch
        C2_Node5[Node 5] --- C2_Switch

        %% 40 Gbit Backplane in Circular Topology
        C2_Node1 ---|40 Gbit| C2_Node2
        C2_Node2 ---|40 Gbit| C2_Node3
        C2_Node3 ---|40 Gbit| C2_Node4
        C2_Node4 ---|40 Gbit| C2_Node5
        C2_Node5 ---|40 Gbit| C2_Node1
    end
    
    %% Cluster 3
    subgraph Cluster3 [**Cluster 3**]
        direction LR
        C3_Switch[Integrated Secure<br>Internet Backplane]
        
        C3_Node1[Node 1] --- C3_Switch
        C3_Node2[Node 2] --- C3_Switch
        C3_Node3[Node 3] --- C3_Switch
        C3_Node4[Node 4] --- C3_Switch
        C3_Node5[Node 5] --- C3_Switch

        %% 40 Gbit Backplane in Circular Topology
        C3_Node1 ---|40 Gbit| C3_Node2
        C3_Node2 ---|40 Gbit| C3_Node3
        C3_Node3 ---|40 Gbit| C3_Node4
        C3_Node4 ---|40 Gbit| C3_Node5
        C3_Node5 ---|40 Gbit| C3_Node1
    end
    

    %% Connect the Ethernet Switches to the Internet
    C1_Switch ---|Internet| Internet[Redundant Mycelium network over the Internet]
    C2_Switch ---|Internet| Internet
    C3_Switch ---|Internet| Internet

```

- Each cluster is a highly efficient cluster in 1 chassis
- Each node is plugeable to be inserted in the cluster chassis
- The cluster has redundant power supply
- The cluster has integrated redundant switch/router for the connection to internet.
- The cluster has a 40 Gbit backplane for the internal communication, acts as a circle and as such is fully redundant. This 40 gbit backplane is used for the communication between the nodes in the cluster for storage and internal networking e.g. for the blockchain.

## capacity of 1 cluster

- **+100,000 passmark for the cluster**
- **480 GB of memory per cluster**
- **40,000 GB of high performance flash per cluster** = storage
- **40 gbit backplane, redundant**
- **2.5 gbit internet routing & firewall infrastructure**
- **+200 TOPS** (Tera Operations Per Second) for AI tasks


Details:

- 96 GB memory per node (5 nodes)
- 2x4 TB high performance nVME flash per node = 5 x 2 x 4
- redundant backplane of 40 gbit between the nodes.
- 2.5 gbit connection to the Internet Backplane in the cluster
- 28 cores per node
- upto 5 5.2 GHz per core
- Intel Arc GPU with up to 12 Xe2 cores per node
- NPU (Neural Processing Unit): Integrated NPU capable of AI acceleration with 45 TOPS (Tera Operations Per Second) for AI tasks
- < 100 watt per node


## Example usecases

- high performance blockchain +1,000 TPS (supports hundreds of MagiCloud Edge Clusters)
- high performance database workloads, super redundant with master, and readonly followers over multiple locations which gives super fast read performance on each edge location.
- windows/linux VM's & Containers with integrated redundant block devices
- integrated Quantum Safe Network and Quantum Safe Storage (archive, fileservers, ...)
- edge applications
- integrates with our MagicCloud Smart Contract for IT system
- AI interference (not high performance though but enough for many workloads)
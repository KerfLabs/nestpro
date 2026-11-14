# Each sheet has its machine

A shop with two different lasers (for example a Siemens-controlled one and a ByStar Fiber) wants to
send each sheet of a job to whichever machine is free, without making a second job. So the machine
belongs to the sheet, not only to the job or to one app-wide setting: new sheets and Nest all take
the job's machine, one sheet can be moved to another, and its automatic cut plan (cut qualities,
leads, cut order, kerf) is then worked out again from that machine's tables. Programs are written
per sheet for that sheet's machine.

**Considered Options**: one machine per job, switched for the whole job at once. Simpler, but a
mixed job would have to be split into two jobs by hand.

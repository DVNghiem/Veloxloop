# Veloxloop Benchmarks

Run at: {{ =datetime.datetime.fromtimestamp(data.run_at).strftime('%a %d %b %Y, %H:%M') }}    
Environment: {{ =benv }} (CPUs: {{ =data.cpu }})    
Python version: {{ =data.pyver }}    
Veloxloop version: {{ =data.veloxloop }}    

### Streams

TCP echo server with `asyncio` streams comparison using 1KB, 10KB and 100KB messages.

{{ _data = data.results["stream"] }}
{{ _ckey = "1" }}
{{ include "./_vs_table_overw.tpl" }}

#### 1KB details

{{ _dkey, _ckey = "1024", "1" }}
{{ include "./_vs_table.tpl" }}

#### 10KB details

{{ _dkey, _ckey = "10240", "1" }}
{{ include "./_vs_table.tpl" }}

#### 100KB details

{{ _dkey, _ckey = "102400", "1" }}
{{ include "./_vs_table.tpl" }}

### Protocol

TCP echo server with `asyncio.Protocol` comparison using 1KB, 10KB and 100KB messages.

{{ _data = data.results["proto"] }}
{{ _ckey = "1" }}
{{ include "./_vs_table_overw.tpl" }}

#### 1KB details

{{ _dkey, _ckey = "1024", "1" }}
{{ include "./_vs_table.tpl" }}

#### 10KB details

{{ _dkey, _ckey = "10240", "1" }}
{{ include "./_vs_table.tpl" }}

#### 100KB details

{{ _dkey, _ckey = "102400", "1" }}
{{ include "./_vs_table.tpl" }}


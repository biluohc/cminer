#!/bin/bash
# https://docs.julialang.org/en/v1.2/manual/faq/#How-do-I-catch-CTRL-C-in-a-script?-1
#=
exec julia --color=yes -e 'include(popfirst!(ARGS))' \
    "${BASH_SOURCE[0]}" "$@"
=#

import Base.parse
import ArgParse.parse_item
using ArgParse, Dates, Sockets, JSON, JSON2

function getsocketaddr(sock::TCPSocket)::String
    (a, p) = getpeername(sock)
    "$a:$p"
end

struct HostPort
    host::String
    port::Int
end

function parse(HostPort, str::AbstractString)
    str2 = split(str, ":")
    if length(str2) != 2
        throw(ArgumentError("invalid str not contains ':' in $str"))
    end
    port = parse(Int, str2[2])
    getalladdrinfo(str2[1])
    HostPort(str2[1], port)
end

function ArgParse.parse_item(::Type{HostPort}, x::AbstractString)
    return parse(HostPort, x)
end

function csx(port::Int, hostport::HostPort, hostport2::Nothing)
    @info("csx listen($port) -> $hostport $hostport2")

    server = listen(port)
    while true
        c = accept(server)
        ca = getsocketaddr(c)
        @info("accept ok: $ca")

        proxyc = connect(hostport.host, hostport.port);
        pa = getsocketaddr(proxyc)
        @info("$ca's proxy connect to server($hostport) ok: $pa")

        chan_up = Channel(4);
        chan_down = Channel(2);

        @async while isopen(c)
            msg = readline(c, keep=true)
            if length(msg) == 0
                @warn("$ca' broken: close it's proxy and chan_up")
                close(chan_up)
                close(proxyc)
            else
                @info("$ca's   up: $msg")
                put!(chan_up, msg)
            end
        end

        @async while isopen(chan_up)
            msg = take!(chan_up)
            write(proxyc, msg)
        end

        @async while isopen(proxyc)
            msg = readline(proxyc, keep=true)
            if length(msg) == 0
                @warn("$ca's proxy broken: close it and it's chan_down")
                close(c)
                close(chan_down)
            else
                @info("$ca's down: $msg")
                put!(chan_down, msg)
            end
        end

        @async while isopen(chan_down)
            msg = take!(chan_down)
            write(c, msg)
        end
    end
end

function csx(port::Int, hostport::HostPort, hostport2::HostPort)
    @info("csx listen($port) -> $hostport, $hostport2")

    server = listen(port)
    while true
        c = accept(server)
        ca = getsocketaddr(c)
        @info("accept ok: $ca")

        proxyc = connect(hostport.host, hostport.port);
        pa = getsocketaddr(proxyc)
        @info("$ca's proxy connect to server($hostport) ok: $pa")

        proxyc2 = connect(hostport2.host, hostport2.port);
        pa2 = getsocketaddr(proxyc2)
        @info("$ca's proxy connect to server2($hostport2) ok: $pa2")

        chan_up = Channel(4)
        chan_down = Channel(2)
        chan_up2 = Channel(4)

        @async while isopen(c)
            msg = readline(c, keep=true)
            if length(msg) == 0
                @warn("$ca' broken: close it's proxy and chan_up")
                close(proxyc)
                close(proxyc2)
                close(chan_up)
                close(chan_up2)
            else
                @info("$ca's   up: $msg")
                put!(chan_up, msg)
                put!(chan_up2, msg)
            end
        end

        @async while isopen(chan_up)
            msg = take!(chan_up)
            write(proxyc, msg)
        end

        @async while isopen(chan_up2)
            msg = take!(chan_up2)
            write(proxyc2, msg)
        end

        @async while isopen(proxyc)
            msg = readline(proxyc, keep=true)
            if length(msg) == 0
                @warn("$ca's proxy broken: close it and it's chan_down")
                close(c)
                close(chan_down)
            else
                @info("$ca's down: $msg")
                put!(chan_down, msg)
            end
        end

        @async while isopen(chan_down)
            msg = take!(chan_down)
            write(c, msg)
        end

        @async while isopen(proxyc2)
            msg = readline(proxyc2, keep=true)
            if length(msg) == 0
                @warn("$ca's proxy2 broken")
            else
                @info("$ca's down2: $msg")
            end
        end
    end
end


# JSON2 neeeds fields order
struct Simulator
    jobExpire::Int
    jobs::Vector{String}
end

function eth_handler(job::AbstractString, req::AbstractString)::String
    id = "1"
    try 
        json = JSON.parse(req)
        id = json["id"]
        method = json["method"]

        if method == "eth_submitWork" || method == "eth_submitHashrate" || method == "eth_submitLogin"
            return """{"id":$id,"jsonrpc":"2.0","result":true}"""
        end
        
        if method == "eth_getWork"
            jobjs = JSON.parse(job)
            jobjs["id"] = id
            return JSON.json(jobjs)
        end
    catch e
        @error("handle req: $req error: $e")
    end

    """{"id":$id,"jsonrpc":"2.0","result":false, "error": "invalid request"}"""
end

function ckb_handler(job::AbstractString, req::AbstractString)::String
    id = nothing
    try
        json = JSON.parse(req)
        id = json["id"]
        method = json["method"]

        if method == "mining.submit"
            return """{"id":$id,"jsonrpc":"2.0","result":true}"""
        end
        
        if method == "mining.subscribe"
            nonce1 = @sprintf("%08x", rand(UInt32))
            return """{"id":$id,"result":[null,"$nonce1",12],"error":null}"""
        end

        if method == "mining.authorize"
            return """{"id":$id,"jsonrpc":"2.0","result":true}
            {"id":null,"method":"mining.set_target","params":["000010c6f7000000000000000000000000000000000000000000000000000000"],"error":null}
            $job"""
        end
    catch e
        @error("handle req: $req error: $e")
    end

    """{"id":$(JSON.json(id)),"jsonrpc":"2.0","result":false, "error": "invalid request"}"""
end

function simulator(port::Int, currency::AbstractString, config::Simulator)
    @info("simulator listen($port) for $currency")

    is_currency = regex -> !isnothing((match(regex, lowercase(currency))))
    handler = if is_currency(r"eth.*")
        handler = eth_handler
    elseif is_currency(r"ckb.*")
        handler = ckb_handler
    else
        error("invalid currency $currency not find handler")
    end

    server = listen(port)

    clients = Dict{String, Channel}()
    job = config.jobs[1]

    @async begin; 
        jobid = 1
        while isopen(server); 
            sleep(config.jobExpire); 
            if jobid + 1 > length(config.jobs)
                jobid = 1
            else
                jobid += 1
            end
            job = config.jobs[jobid]
            @warn("broadcast job $jobid for $(length(clients)) clients: $job")
            deletec = 0
            for (ca, client) in clients
                if isopen(client)
                    put!(client, job)
                else
                    deletec += 1
                   delete!(clients, ca)
                end
            end
            @warn("broadcast job $jobid ok, delete $deletec clients")
        end
    end

    while true
        c = accept(server)
        ca = getsocketaddr(c)
        @info("accept ok: $ca")

        chan_down = Channel(8)

        @async while isopen(c)
            msg = readline(c, keep=true)
            if length(msg) == 0
                @warn("$ca' broken: close it's chan_down")
                close(chan_down)
            else
                msg = strip(msg)
                @info("$ca's   req_: $msg")
                resp = handler(job, msg)
                @info("$ca's   resp: $resp")
                put!(chan_down, resp)
            end
        end

        @async while isopen(chan_down)
            msg = take!(chan_down)
            write(c, msg)
            write(c, "\r\n")
        end

        get!(clients, ca, chan_down)
    end
end


function parse_args(args::Vector{String})
    as = ArgParseSettings()

    @add_arg_table as begin
        "--port", "-p"
            help = "the port of serve"
            arg_type = Int
            default = 2510
        "--num", "-n"
            help = "the num of additonal connections"
            arg_type = Int
            default = 0
        "--currency", "-c"
            help = "the currency for pool simulator"
            arg_type = String
            default = ""
        "--config"
            help="the config for pool simulator"
            arg_type = String
            default = "goproxy.json"
        "arg1"
            help = "the address for pool"
            arg_type = HostPort
            required = true
        "arg2"
            help = "the address for optional pool2"
            arg_type = HostPort
            required = false
    end

    return ArgParse.parse_args(as)
end

@warn("ARGS: $ARGS")
args = parse_args(ARGS)
@warn("args: $(args |> JSON.json)")

port = args["port"]
wa = args["arg1"]
wa2 = args["arg2"]
currency = args["currency"]
config = args["config"]

try
    if currency == "" 
        csx(port, wa, wa2)
    else 
        configcs = JSON.parse(read(config, String))[currency] |> JSON.json
        configc = JSON2.read(configcs, Simulator)
        simulator(port, currency, configc)
    end
catch e
    @error("listen($port, $wa, $wa2) failed: $e")
    return
end


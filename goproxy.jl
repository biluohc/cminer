#!/bin/bash
# https://docs.julialang.org/en/v1.2/manual/faq/#How-do-I-catch-CTRL-C-in-a-script?-1
#=
exec julia --color=yes -e 'include(popfirst!(ARGS))' \
    "${BASH_SOURCE[0]}" "$@"
=#

import Base.parse
import ArgParse.parse_item
using ArgParse, Dates, Sockets, JSON

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

try
    csx(port, wa, wa2)
catch e
    @error("ctx($port, $wa, $wa2) failed: $e")
    return
end


# type Simulator struct {
# 	Methods   map[string]string `json:"methods"`
# 	Jobs      []string          `json:"jobs"`
# 	JobExpire uint64            `json:"jobExpire"`
# }

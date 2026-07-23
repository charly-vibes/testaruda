module PkgB

using ..PkgA

export double, square

function double(x::Int)::Int
    return 2x
end

function square(x::Int)::Int
    return x * x
end

end # module PkgB
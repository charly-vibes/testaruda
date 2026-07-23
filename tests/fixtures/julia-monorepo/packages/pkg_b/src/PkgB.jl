module PkgB

export double, square, add

function double(x::Int)::Int
    return 2x
end

function square(x::Int)::Int
    return x * x
end

# Defined separately from PkgA.add to avoid cross-module dependency
function add(x::Int, y::Int)::Int
    return x + y
end

end # module PkgB
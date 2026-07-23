module PkgA

export greet

function greet(name::String)::String
    return "Hello from PkgA, $name!"
end

function add(x::Int, y::Int)::Int
    return x + y
end

end # module PkgA